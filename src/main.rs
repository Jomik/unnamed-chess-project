#[cfg(target_os = "espidf")]
fn main() {
    use esp_idf_svc::hal::adc::oneshot::AdcDriver;
    use esp_idf_svc::hal::peripherals::Peripherals;
    use shakmaty::{Bitboard, ByColor, Color};
    use unnamed_chess_project::esp32::config::{LedPalette, SensorConfig};
    use unnamed_chess_project::esp32::{Esp32LedDisplay, Esp32PieceSensor};
    use unnamed_chess_project::feedback::{BoardFeedback, FeedbackSource, compute_feedback};
    use unnamed_chess_project::game_logic::GameEngine;
    use unnamed_chess_project::opponent::{EmbeddedEngine, Opponent};
    use unnamed_chess_project::recovery::recovery_feedback;
    use unnamed_chess_project::setup::setup_feedback;
    use unnamed_chess_project::{BoardDisplay, PieceSensor};

    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().expect("failed to take peripherals");

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
        SensorConfig {
            baseline_mv: 1440,
            threshold_mv: 200,
            settle_delay_ms: 2,
        },
    )
    .expect("failed to init sensor");

    let mut display = Esp32LedDisplay::new(peripherals.pins.gpio2, LedPalette::default())
        .expect("failed to init LED display");

    log::info!("Waiting for starting position...");
    loop {
        let positions = match sensor.read_positions() {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Sensor read failed: {e}");
                esp_idf_svc::hal::delay::FreeRtos::delay_ms(100);
                continue;
            }
        };

        match setup_feedback(&positions) {
            Some(fb) => {
                if let Err(e) = display.show(&fb) {
                    log::warn!("LED update failed: {e}");
                }
            }
            None => break,
        }

        esp_idf_svc::hal::delay::FreeRtos::delay_ms(50);
    }
    log::info!("Starting position detected");

    if let Err(e) = display.show(&BoardFeedback::default()) {
        log::warn!("LED clear failed: {e}");
    }

    let mut engine = GameEngine::new();
    let mut opponent = EmbeddedEngine::new();
    let mut prev = ByColor {
        white: Bitboard::EMPTY,
        black: Bitboard::EMPTY,
    };
    log::info!("Game loop started");

    loop {
        let positions = match sensor.read_positions() {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Sensor read failed: {e}");
                esp_idf_svc::hal::delay::FreeRtos::delay_ms(100);
                continue;
            }
        };

        let white_added = positions.white & !prev.white;
        let white_removed = prev.white & !positions.white;
        let black_added = positions.black & !prev.black;
        let black_removed = prev.black & !positions.black;

        for sq in white_added {
            log::info!("+ {sq} white");
        }
        for sq in white_removed {
            log::info!("- {sq} white");
        }
        for sq in black_added {
            log::info!("+ {sq} black");
        }
        for sq in black_removed {
            log::info!("- {sq} black");
        }
        prev = positions;

        let state = engine.tick(positions);

        // When it's black's turn and the board is idle, let the computer play.
        if engine.turn() == Color::Black
            && engine.reconciliation().is_none()
            && state.lifted_piece().is_none()
            && state.outcome().is_none()
        {
            opponent.start_thinking(engine.position());
            if let Some(mv) = opponent.poll_move() {
                match engine.apply_opponent_move(&mv) {
                    Ok(()) => log::info!("Computer plays: {mv}"),
                    Err(e) => log::warn!("Computer move failed: {e}"),
                }
            }
        }

        let feedback = compute_feedback(&state);

        // When idle (no move in progress), check if the physical board
        // diverges from the game state and guide the user to fix it.
        let feedback = if feedback.is_empty() {
            recovery_feedback(&engine.expected_positions(), &positions).unwrap_or(feedback)
        } else {
            feedback
        };

        if let Err(e) = display.show(&feedback) {
            log::warn!("LED update failed: {e}");
        }

        esp_idf_svc::hal::delay::FreeRtos::delay_ms(50);
    }
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    unnamed_chess_project::mock::run_interactive_terminal();
}
