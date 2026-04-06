#[cfg(target_os = "espidf")]
fn main() {
    use esp_idf_svc::eventloop::EspSystemEventLoop;
    use esp_idf_svc::hal::adc::oneshot::AdcDriver;
    use esp_idf_svc::hal::delay::FreeRtos;
    use esp_idf_svc::hal::peripherals::Peripherals;
    use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvsPartition, NvsCustom};
    use esp_idf_svc::wifi::AuthMethod;
    use shakmaty::Color;
    use unnamed_chess_project::ble_protocol::{
        BleCommand, CommandResult, CommandSource, GameState, GameStatus, LichessStatus,
        PlayerConfig, UNSET_BYTE, WifiAuthMode, WifiStatus,
    };
    use unnamed_chess_project::esp32::config::{LedPalette, SensorCalibration, SensorConfig};
    use unnamed_chess_project::esp32::{
        Esp32LedDisplay, Esp32LichessClient, Esp32PieceSensor, WifiConnection, start_ble,
    };
    use unnamed_chess_project::lichess::{LichessConfig, spawn_lichess_opponent};
    use unnamed_chess_project::session::GameSession;
    use unnamed_chess_project::{BoardDisplay, PieceSensor};

    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().expect("failed to take peripherals");

    let sys_loop = EspSystemEventLoop::take().expect("failed to take system event loop");
    let nvs_partition = EspDefaultNvsPartition::take().expect("failed to take NVS partition");
    let mut modem = Some(peripherals.modem);

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

    let mut wifi_connection: Option<WifiConnection<'_>> = None;
    let mut lichess_token: Option<String> = None;

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

                    // Phase 1: Spawn Lichess opponents (blocking, up to 90s each)
                    // before waiting for starting position.
                    let mut white_lichess: Option<Box<dyn unnamed_chess_project::player::Player>> =
                        None;
                    let mut black_lichess: Option<Box<dyn unnamed_chess_project::player::Player>> =
                        None;

                    if let PlayerConfig::LichessAi { level } = w_config {
                        if wifi_connection.is_none() {
                            notifier.notify_command_result(&CommandResult::error(
                                CommandSource::StartGame,
                                "WiFi not connected",
                            ));
                            continue;
                        }
                        let Some(ref token) = lichess_token else {
                            notifier.notify_command_result(&CommandResult::error(
                                CommandSource::StartGame,
                                "Lichess token not set",
                            ));
                            continue;
                        };
                        let client = Esp32LichessClient::new(token.clone());
                        let config = LichessConfig {
                            level: *level,
                            clock_limit: 10800,
                            clock_increment: 180,
                        };
                        match spawn_lichess_opponent(client, config, |f| {
                            std::thread::Builder::new()
                                .stack_size(8192)
                                .spawn(f)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        }) {
                            Ok(opponent) => {
                                white_lichess = Some(Box::new(opponent));
                            }
                            Err(e) => {
                                log::error!("Failed to spawn white Lichess opponent: {e}");
                                notifier.notify_command_result(&CommandResult::error(
                                    CommandSource::StartGame,
                                    format!("{e}"),
                                ));
                                continue;
                            }
                        }
                    }

                    if let PlayerConfig::LichessAi { level } = b_config {
                        if wifi_connection.is_none() {
                            notifier.notify_command_result(&CommandResult::error(
                                CommandSource::StartGame,
                                "WiFi not connected",
                            ));
                            continue;
                        }
                        let Some(ref token) = lichess_token else {
                            notifier.notify_command_result(&CommandResult::error(
                                CommandSource::StartGame,
                                "Lichess token not set",
                            ));
                            continue;
                        };
                        let client = Esp32LichessClient::new(token.clone());
                        let config = LichessConfig {
                            level: *level,
                            clock_limit: 10800,
                            clock_increment: 180,
                        };
                        match spawn_lichess_opponent(client, config, |f| {
                            std::thread::Builder::new()
                                .stack_size(8192)
                                .spawn(f)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        }) {
                            Ok(opponent) => {
                                black_lichess = Some(Box::new(opponent));
                            }
                            Err(e) => {
                                log::error!("Failed to spawn black Lichess opponent: {e}");
                                notifier.notify_command_result(&CommandResult::error(
                                    CommandSource::StartGame,
                                    format!("{e}"),
                                ));
                                continue;
                            }
                        }
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

                    // Phase 2: Create Human/Embedded players (need initial_positions).
                    // Lichess players were already spawned in phase 1.
                    let white_player: Box<dyn unnamed_chess_project::player::Player> =
                        match white_lichess {
                            Some(p) => p,
                            None => create_player(w_config, initial_positions),
                        };
                    let black_player: Box<dyn unnamed_chess_project::player::Player> =
                        match black_lichess {
                            Some(p) => p,
                            None => create_player(b_config, initial_positions),
                        };

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
                        if s.resign(color) {
                            log::info!("{color:?} resigns");
                            let state = s.game_state();
                            notifier.notify_command_result(&CommandResult::success(
                                CommandSource::MatchControl,
                            ));
                            notifier.notify_game_state(&state);
                            prev_game_state = Some(state);
                        } else {
                            log::warn!("Resign rejected for {color:?}");
                            notifier.notify_command_result(&CommandResult::error(
                                CommandSource::MatchControl,
                                "cannot resign for non-human player",
                            ));
                        }
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
                    notifier.notify_wifi_status(&WifiStatus::connecting());

                    let Some(mdm) = modem.take() else {
                        log::error!("WiFi modem already consumed");
                        notifier.notify_wifi_status(&WifiStatus::failed(
                            "WiFi modem unavailable, reboot to retry",
                        ));
                        continue;
                    };

                    let auth_method = match config.auth_mode {
                        WifiAuthMode::Open => AuthMethod::None,
                        WifiAuthMode::Wpa2 => AuthMethod::WPA2Personal,
                        WifiAuthMode::Wpa3 => AuthMethod::WPA3Personal,
                    };

                    match WifiConnection::connect(
                        mdm,
                        sys_loop.clone(),
                        nvs_partition.clone(),
                        &config.ssid,
                        &config.password,
                        auth_method,
                    ) {
                        Ok(conn) => {
                            log::info!("WiFi connected successfully");
                            wifi_connection = Some(conn);
                            notifier.notify_wifi_status(&WifiStatus::connected());
                        }
                        Err(e) => {
                            log::error!("WiFi connection failed: {e}");
                            notifier.notify_wifi_status(&WifiStatus::failed(e.to_string()));
                        }
                    }
                }
                BleCommand::SetLichessToken(token) => {
                    log::info!("Lichess token received ({} chars)", token.len());
                    lichess_token = Some(token);
                    notifier.notify_lichess_status(&LichessStatus::connected());
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
