#[cfg(target_os = "espidf")]
enum BoardState {
    Idle,
    AwaitingPieces {
        white: unnamed_chess_project::board_api::PlayerType,
        black: unnamed_chess_project::board_api::PlayerType,
    },
    InProgress {
        session: unnamed_chess_project::session::GameSession,
        white_tx: Option<std::sync::mpsc::Sender<shakmaty::Move>>,
        black_tx: Option<std::sync::mpsc::Sender<shakmaty::Move>>,
    },
}

#[cfg(target_os = "espidf")]
fn main() {
    use esp_idf_svc::hal::adc::oneshot::AdcDriver;
    use esp_idf_svc::hal::delay::FreeRtos;
    use esp_idf_svc::hal::peripherals::Peripherals;
    use esp_idf_svc::nvs::{EspNvsPartition, NvsCustom};
    use shakmaty::{Color, Position};
    use unnamed_chess_project::ble_protocol::{
        BleCommand, CommandResult, CommandSource, ErrorCode,
    };
    use unnamed_chess_project::board_api;
    use unnamed_chess_project::esp32::config::{LedPalette, SensorCalibration, SensorConfig};
    use unnamed_chess_project::esp32::{Esp32LedDisplay, Esp32PieceSensor, start_ble};
    use unnamed_chess_project::feedback::BoardFeedback;
    use unnamed_chess_project::session::GameSession;
    use unnamed_chess_project::setup::setup_feedback;
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

    let mut board_state = BoardState::Idle;
    let mut prev_positions = None;
    let mut prev_game_state: Option<board_api::GameStatus> = None;

    log::info!("Entering BLE command loop");

    loop {
        while let Some(cmd) = commands.try_recv() {
            match cmd {
                BleCommand::StartGame { white, black } => {
                    if !matches!(board_state, BoardState::Idle) {
                        notifier.notify_command_result(&CommandResult::error(
                            CommandSource::StartGame,
                            ErrorCode::GameAlreadyInProgress,
                        ));
                        continue;
                    }
                    notifier
                        .notify_command_result(&CommandResult::success(CommandSource::StartGame));
                    notifier.update_player_type(Color::White, white);
                    notifier.update_player_type(Color::Black, black);
                    notifier.notify_game_status(&board_api::GameStatus::AwaitingPieces);
                    board_state = BoardState::AwaitingPieces { white, black };
                    log::info!("Waiting for starting position...");
                    break; // State mutation
                }
                BleCommand::CancelGame => match board_state {
                    BoardState::Idle => {
                        notifier.notify_command_result(&CommandResult::error(
                            CommandSource::MatchControl,
                            ErrorCode::NoGameInProgress,
                        ));
                    }
                    _ => {
                        board_state = BoardState::Idle;
                        notifier.notify_command_result(&CommandResult::success(
                            CommandSource::MatchControl,
                        ));
                        notifier.notify_game_status(&board_api::GameStatus::Idle);
                        notifier.reset_player_types();
                        notifier.reset_position();
                        notifier.reset_last_move();
                        log::info!("Game cancelled");
                        break; // State mutation
                    }
                },
                BleCommand::SubmitMove { uci } => {
                    let BoardState::InProgress {
                        ref session,
                        ref white_tx,
                        ref black_tx,
                    } = board_state
                    else {
                        notifier.notify_command_result(&CommandResult::error(
                            CommandSource::SubmitMove,
                            ErrorCode::NoGameInProgress,
                        ));
                        continue;
                    };
                    let turn = session.position().turn();
                    let tx = match turn {
                        Color::White => white_tx,
                        Color::Black => black_tx,
                    };
                    let Some(tx) = tx else {
                        // Current player is Human, not Remote — reject
                        notifier.notify_command_result(&CommandResult::error(
                            CommandSource::SubmitMove,
                            ErrorCode::NotYourTurn,
                        ));
                        continue;
                    };
                    // Parse and validate the UCI move
                    match parse_uci_move(session.position(), &uci) {
                        Ok(mv) => {
                            if tx.send(mv).is_err() {
                                log::warn!("SubmitMove: channel closed, receiver dropped");
                                notifier.notify_command_result(&CommandResult::error(
                                    CommandSource::SubmitMove,
                                    ErrorCode::NoGameInProgress,
                                ));
                            } else {
                                notifier.notify_command_result(&CommandResult::success(
                                    CommandSource::SubmitMove,
                                ));
                            }
                        }
                        Err(_) => {
                            notifier.notify_command_result(&CommandResult::error(
                                CommandSource::SubmitMove,
                                ErrorCode::IllegalMove,
                            ));
                        }
                    }
                    break; // Force tick before processing next command
                }
                BleCommand::Resign { color } => {
                    if let BoardState::InProgress {
                        ref mut session,
                        ref white_tx,
                        ref black_tx,
                    } = board_state
                    {
                        // Check that the side being resigned is Human (not Remote)
                        let is_remote = match color {
                            Color::White => white_tx.is_some(),
                            Color::Black => black_tx.is_some(),
                        };
                        if is_remote {
                            notifier.notify_command_result(&CommandResult::error(
                                CommandSource::MatchControl,
                                ErrorCode::CannotResignForRemotePlayer,
                            ));
                        } else if session.resign(color) {
                            log::info!("{color:?} resigns");
                            notifier.notify_command_result(&CommandResult::success(
                                CommandSource::MatchControl,
                            ));
                            notifier.notify_game_status(&session.game_state());
                        } else {
                            // Should not happen if is_remote check is correct, but handle gracefully
                            notifier.notify_command_result(&CommandResult::error(
                                CommandSource::MatchControl,
                                ErrorCode::InvalidCommand,
                            ));
                        }
                        break; // Force tick after state mutation
                    } else {
                        notifier.notify_command_result(&CommandResult::error(
                            CommandSource::MatchControl,
                            ErrorCode::NoGameInProgress,
                        ));
                    }
                }
            }
        }

        if let BoardState::AwaitingPieces {
            ref white,
            ref black,
        } = board_state
        {
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
                }
                None => {
                    // Starting position detected — create session
                    if let Err(e) = display.show(&BoardFeedback::default()) {
                        log::warn!("LED clear failed: {e}");
                    }
                    let initial = match sensor.read_positions() {
                        Ok(p) => p,
                        Err(e) => {
                            log::error!("Initial sensor read failed: {e}");
                            board_state = BoardState::Idle;
                            notifier.notify_game_status(&board_api::GameStatus::Idle);
                            continue;
                        }
                    };
                    let (white_player, w_tx) = create_player(*white, initial);
                    let (black_player, b_tx) = create_player(*black, initial);
                    let new_session = GameSession::new(white_player, black_player);
                    notifier.notify_game_status(&new_session.game_state());
                    // Set initial position FEN
                    let fen = shakmaty::fen::Fen::from_position(
                        new_session.position(),
                        shakmaty::EnPassantMode::Legal,
                    )
                    .to_string();
                    notifier.update_position(&fen);
                    prev_game_state = Some(new_session.game_state());
                    prev_positions = Some(initial);
                    board_state = BoardState::InProgress {
                        session: new_session,
                        white_tx: w_tx,
                        black_tx: b_tx,
                    };
                    log::info!("Starting position detected, game started");
                }
            }
        }

        if let BoardState::InProgress {
            ref mut session, ..
        } = board_state
        {
            // Check game-over FIRST (handles resign from command processing above)
            if session.is_game_over() {
                log::info!("Game over: {:?}", session.game_state());
                board_state = BoardState::Idle;
                prev_positions = None;
                prev_game_state = None;
                notifier.reset_player_types();
                // Don't reset position/last_move — keep them so the app can read the final state
                FreeRtos::delay_ms(50);
                continue;
            }

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

            let result = session.tick(positions);
            if let Some(ref mv) = result.last_move {
                use shakmaty::uci::UciMove;
                log::info!("Move played: {mv}");
                // The player who just moved is the opposite of current turn (turn already advanced)
                let mover = !session.position().turn();
                let uci = UciMove::from_move(*mv, shakmaty::CastlingMode::Standard).to_string();
                notifier.notify_move_played(mover, &uci);
                notifier.update_last_move(mover, &uci);
            }

            if result.last_move.is_some() {
                let fen = shakmaty::fen::Fen::from_position(
                    session.position(),
                    shakmaty::EnPassantMode::Legal,
                )
                .to_string();
                notifier.update_position(&fen);
            }

            if let Err(e) = display.show(&result.feedback) {
                log::warn!("LED update failed: {e}");
            }

            let current_status = session.game_state();
            let state_changed = prev_game_state.as_ref() != Some(&current_status);
            if state_changed {
                notifier.notify_game_status(&current_status);
                prev_game_state = Some(current_status);
            }
        }

        FreeRtos::delay_ms(50);
    }
}

#[cfg(target_os = "espidf")]
#[derive(Debug, thiserror::Error)]
enum MoveParseError {
    #[error("invalid UCI notation")]
    InvalidUci,
    #[error("illegal move in current position")]
    IllegalMove,
}

#[cfg(target_os = "espidf")]
fn parse_uci_move(position: &shakmaty::Chess, uci: &str) -> Result<shakmaty::Move, MoveParseError> {
    use shakmaty::uci::UciMove;
    use std::str::FromStr;
    let uci_move = UciMove::from_str(uci).map_err(|_| MoveParseError::InvalidUci)?;
    uci_move
        .to_move(position)
        .map_err(|_| MoveParseError::IllegalMove)
}

#[cfg(target_os = "espidf")]
fn create_player(
    player_type: unnamed_chess_project::board_api::PlayerType,
    initial_positions: shakmaty::ByColor<shakmaty::Bitboard>,
) -> (
    Box<dyn unnamed_chess_project::player::Player>,
    Option<std::sync::mpsc::Sender<shakmaty::Move>>,
) {
    use unnamed_chess_project::board_api;
    use unnamed_chess_project::player::{HumanPlayer, RemotePlayer};

    match player_type {
        board_api::PlayerType::Human => (Box::new(HumanPlayer::new(initial_positions)), None),
        board_api::PlayerType::Remote => {
            let (tx, rx) = std::sync::mpsc::channel();
            (Box::new(RemotePlayer::new(rx)), Some(tx))
        }
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
