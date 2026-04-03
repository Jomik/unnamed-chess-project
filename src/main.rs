#[cfg(target_os = "espidf")]
fn main() {
    use esp_idf_svc::hal::adc::oneshot::AdcDriver;
    use esp_idf_svc::hal::delay::FreeRtos;
    use esp_idf_svc::hal::peripherals::Peripherals;
    use esp_idf_svc::nvs::{EspNvsPartition, NvsCustom};
    use shakmaty::Color;
    use unnamed_chess_project::ble_protocol::{
        BleCommand, CommandResult, CommandSource, GameState, GameStatus, PlayerConfig, UNSET_BYTE,
    };
    use unnamed_chess_project::esp32::config::{LedPalette, SensorCalibration, SensorConfig};
    use unnamed_chess_project::esp32::{Esp32LedDisplay, Esp32PieceSensor, start_ble};
    use unnamed_chess_project::session::GameSession;
    use unnamed_chess_project::{BoardDisplay, PieceSensor};

    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().expect("failed to take peripherals");

    let mut display = Esp32LedDisplay::new(peripherals.pins.gpio2, LedPalette::default())
        .expect("failed to init LED display");

    // Load sensor calibration from the dedicated cal partition (survives erase-nvs)
    let cal_partition =
        EspNvsPartition::<NvsCustom>::take("cal").expect("failed to take cal NVS partition");
    let sensor_config = match SensorCalibration::load(&cal_partition) {
        Ok(Some(cal)) => {
            log::info!(
                "Using NVS calibration: baseline={}mV, threshold={}mV",
                cal.baseline_mv,
                cal.threshold_mv
            );
            SensorConfig {
                baseline_mv: cal.baseline_mv,
                threshold_mv: cal.threshold_mv,
                ..SensorConfig::default()
            }
        }
        Ok(None) => {
            log::info!("No sensor calibration in NVS, using defaults");
            SensorConfig::default()
        }
        Err(e) => {
            log::warn!("NVS calibration read failed: {e} -- using defaults");
            SensorConfig::default()
        }
    };

    let adc_driver = AdcDriver::new(peripherals.adc1).expect("failed to init ADC1");
    let mut sensor = Esp32PieceSensor::new(
        &adc_driver,
        peripherals.pins.gpio4,
        peripherals.pins.gpio5,
        peripherals.pins.gpio6,
        peripherals.pins.gpio7,
        peripherals.pins.gpio9,
        peripherals.pins.gpio10,
        peripherals.pins.gpio11,
        peripherals.pins.gpio12,
        sensor_config,
    )
    .expect("failed to init sensor");

    let (commands, notifier) = start_ble().expect("failed to start BLE server");

    let mut white_config: Option<PlayerConfig> = None;
    let mut black_config: Option<PlayerConfig> = None;
    let mut session: Option<GameSession> = None;
    let mut prev_positions = None;
    let mut prev_game_state: Option<GameState> = None;

    log::info!("Entering BLE command loop");

    loop {
        while let Some(cmd) = commands.try_recv() {
            match cmd {
                BleCommand::SetWhitePlayer(config) => {
                    log::info!("White player set to: {config:?}");
                    let encoded = config.encode();
                    notifier.update_player_config(Color::White, &encoded);
                    white_config = Some(config);
                }
                BleCommand::SetBlackPlayer(config) => {
                    log::info!("Black player set to: {config:?}");
                    let encoded = config.encode();
                    notifier.update_player_config(Color::Black, &encoded);
                    black_config = Some(config);
                }
                BleCommand::StartGame => {
                    if session.is_some() {
                        log::warn!("StartGame received while game is active");
                        notifier.notify_command_result(&CommandResult::error(
                            CommandSource::StartGame,
                            "game already in progress",
                        ));
                        continue;
                    }

                    let (Some(w_config), Some(b_config)) = (&white_config, &black_config) else {
                        log::warn!("StartGame received but players not configured");
                        notifier.notify_command_result(&CommandResult::error(
                            CommandSource::StartGame,
                            "both players must be configured",
                        ));
                        continue;
                    };

                    if matches!(w_config, PlayerConfig::LichessAi { .. }) {
                        notifier.notify_command_result(&CommandResult::error(
                            CommandSource::StartGame,
                            "Lichess AI not supported",
                        ));
                        continue;
                    }
                    if matches!(b_config, PlayerConfig::LichessAi { .. }) {
                        notifier.notify_command_result(&CommandResult::error(
                            CommandSource::StartGame,
                            "Lichess AI not supported",
                        ));
                        continue;
                    }

                    // Acknowledge the command and notify AwaitingPieces before
                    // entering the blocking setup loop.
                    notifier
                        .notify_command_result(&CommandResult::success(CommandSource::StartGame));
                    let awaiting_state = GameState {
                        status: GameStatus::AwaitingPieces,
                        turn: Color::White,
                    };
                    notifier.notify_game_state(&awaiting_state);

                    log::info!("Waiting for starting position...");
                    let initial_positions =
                        match wait_for_starting_position(&mut sensor, &mut display) {
                            Ok(p) => p,
                            Err(e) => {
                                log::error!("Initial sensor read failed: {e}");
                                // Reset to Idle since setup failed.
                                let idle_state = GameState::idle();
                                notifier.notify_command_result(&CommandResult::error(
                                    CommandSource::StartGame,
                                    "sensor read failed",
                                ));
                                notifier.notify_game_state(&idle_state);
                                prev_game_state = Some(idle_state);
                                continue;
                            }
                        };
                    log::info!("Starting position detected");

                    let white_player = create_player(w_config, initial_positions);
                    let black_player = create_player(b_config, initial_positions);

                    let new_session = GameSession::new(white_player, black_player);
                    let state = new_session.game_state();
                    notifier.notify_game_state(&state);
                    prev_game_state = Some(state);

                    session = Some(new_session);
                    prev_positions = Some(initial_positions);

                    log::info!("Game started");
                }
                BleCommand::Resign { color } => {
                    if let Some(ref mut s) = session {
                        log::info!("{color:?} resigns");
                        s.resign(color);
                        let state = s.game_state();
                        notifier.notify_command_result(&CommandResult::success(
                            CommandSource::MatchControl,
                        ));
                        notifier.notify_game_state(&state);
                        prev_game_state = Some(state);
                    } else {
                        log::warn!("Resign received but no game is active");
                        notifier.notify_command_result(&CommandResult::error(
                            CommandSource::MatchControl,
                            "no game in progress",
                        ));
                    }
                }
                BleCommand::ConfigureWifi(config) => {
                    log::info!("WiFi config received: ssid={}", config.ssid);
                    // TODO(Phase 2): connect to WiFi using config
                }
                BleCommand::SetLichessToken(token) => {
                    log::info!("Lichess token received ({} chars)", token.len());
                    // TODO(Phase 2): validate and store Lichess token
                }
            }
        }

        if let Some(ref mut s) = session {
            let positions = match sensor.read_positions() {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("Sensor read failed: {e}");
                    FreeRtos::delay_ms(100);
                    continue;
                }
            };

            log_sensor_changes(prev_positions, positions);
            prev_positions = Some(positions);

            let result = s.tick(positions);

            if let Some(mv) = &result.last_move {
                log::info!("Move played: {mv}");
            }

            if let Err(e) = display.show(&result.feedback) {
                log::warn!("LED update failed: {e}");
            }

            let current_state = s.game_state();
            let state_changed = prev_game_state.as_ref() != Some(&current_state);
            if state_changed {
                notifier.notify_game_state(&current_state);
                prev_game_state = Some(current_state);
            }

            if s.is_game_over() {
                let final_state = s.game_state();
                log::info!("Game over: {:?}", final_state.status);

                session = None;
                white_config = None;
                black_config = None;
                prev_positions = None;
                prev_game_state = None;

                notifier.update_player_config(Color::White, &[UNSET_BYTE]);
                notifier.update_player_config(Color::Black, &[UNSET_BYTE]);
            }
        }

        FreeRtos::delay_ms(50);
    }
}

#[cfg(target_os = "espidf")]
fn wait_for_starting_position<S, D>(
    sensor: &mut S,
    display: &mut D,
) -> Result<shakmaty::ByColor<shakmaty::Bitboard>, S::Error>
where
    S: unnamed_chess_project::PieceSensor,
    D: unnamed_chess_project::BoardDisplay,
{
    use esp_idf_svc::hal::delay::FreeRtos;
    use unnamed_chess_project::feedback::BoardFeedback;
    use unnamed_chess_project::setup::setup_feedback;

    loop {
        let positions = match sensor.read_positions() {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Sensor read failed: {e}");
                FreeRtos::delay_ms(100);
                continue;
            }
        };
        match setup_feedback(&positions) {
            Some(fb) => {
                if let Err(e) = display.show(&fb) {
                    log::warn!("LED update failed: {e}");
                }
                FreeRtos::delay_ms(50);
            }
            None => {
                if let Err(e) = display.show(&BoardFeedback::default()) {
                    log::warn!("LED clear failed: {e}");
                }
                return sensor.read_positions();
            }
        }
    }
}

/// Panics on `LichessAi` -- callers must reject it beforehand.
#[cfg(target_os = "espidf")]
fn create_player(
    config: &unnamed_chess_project::ble_protocol::PlayerConfig,
    initial_positions: shakmaty::ByColor<shakmaty::Bitboard>,
) -> Box<dyn unnamed_chess_project::player::Player> {
    use unnamed_chess_project::ble_protocol::PlayerConfig;
    use unnamed_chess_project::player::{EmbeddedEngine, HumanPlayer};

    match config {
        PlayerConfig::Human => Box::new(HumanPlayer::new(initial_positions)),
        PlayerConfig::Embedded => Box::new(EmbeddedEngine::new(unsafe {
            esp_idf_svc::sys::esp_random()
        })),
        PlayerConfig::LichessAi { .. } => unreachable!(),
    }
}

#[cfg(target_os = "espidf")]
fn log_sensor_changes(
    prev: Option<shakmaty::ByColor<shakmaty::Bitboard>>,
    current: shakmaty::ByColor<shakmaty::Bitboard>,
) {
    let Some(prev) = prev else { return };

    for sq in current.white & !prev.white {
        log::debug!("+ {sq} white");
    }
    for sq in prev.white & !current.white {
        log::debug!("- {sq} white");
    }
    for sq in current.black & !prev.black {
        log::debug!("+ {sq} black");
    }
    for sq in prev.black & !current.black {
        log::debug!("- {sq} black");
    }
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    eprintln!(
        "This binary targets ESP32. Use `just flash` to run on hardware, or `just test` to run tests."
    );
}
