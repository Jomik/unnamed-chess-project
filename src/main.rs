#[cfg(target_os = "espidf")]
fn main() {
    use esp_idf_svc::hal::adc::oneshot::AdcDriver;
    use esp_idf_svc::hal::peripherals::Peripherals;
    use shakmaty::{Bitboard, ByColor};
    use unnamed_chess_project::esp32::config::{LedPalette, SensorConfig};
    use unnamed_chess_project::esp32::{Esp32LedDisplay, Esp32PieceSensor};
    use unnamed_chess_project::feedback::compute_feedback;
    use unnamed_chess_project::game_logic::GameEngine;
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

    let mut engine = GameEngine::new();
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
        let feedback = compute_feedback(&state);

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
