#[cfg(target_os = "espidf")]
use unnamed_chess_project::esp32::{Esp32LedDisplay, Esp32PieceSensor};

#[cfg(target_os = "espidf")]
fn main() {
    use esp_idf_svc::hal::adc::oneshot::AdcDriver;
    use esp_idf_svc::hal::peripherals::Peripherals;
    use unnamed_chess_project::esp32::config::{LedPalette, Rgb8, SensorConfig};

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

    let diag_palette = LedPalette {
        off: Rgb8::new(0, 0, 0),
        destination: Rgb8::new(15, 15, 15),
        capture: Rgb8::new(15, 0, 15),
        origin: Rgb8::new(0, 0, 0),
        check: Rgb8::new(0, 0, 0),
        checker: Rgb8::new(0, 0, 0),
        victory: Rgb8::new(0, 0, 0),
        stalemate: Rgb8::new(0, 0, 0),
    };

    let mut display = Esp32LedDisplay::new(peripherals.pins.gpio2, diag_palette)
        .expect("failed to init LED display");

    log::info!("=== Diagnostics: LED sweep ===");
    led_sweep(&mut display);

    log::info!("=== Diagnostics: Sensor visualization ===");
    sensor_visualization(&mut sensor, &mut display);
}

/// Light one square at a time in a snake pattern (a1→h1, h2→a2, …) and back,
/// to verify LED wiring without drawing excessive current.
#[cfg(target_os = "espidf")]
fn led_sweep(display: &mut Esp32LedDisplay) {
    use esp_idf_svc::hal::delay::FreeRtos;
    use shakmaty::{File, Rank, Square};
    use unnamed_chess_project::BoardDisplay;
    use unnamed_chess_project::feedback::{BoardFeedback, SquareFeedback};

    let snake_order: Vec<Square> = (0..8u32)
        .flat_map(|rank_idx| {
            let rank = Rank::new(rank_idx);
            let files: Vec<u32> = if rank_idx % 2 == 0 {
                (0..8).collect()
            } else {
                (0..8).rev().collect()
            };
            files
                .into_iter()
                .map(move |file_idx| Square::from_coords(File::new(file_idx), rank))
        })
        .collect();

    for &sq in &snake_order {
        let mut fb = BoardFeedback::new();
        fb.set(sq, SquareFeedback::Destination);
        if let Err(e) = display.show(&fb) {
            log::warn!("LED sweep {sq} failed: {e}");
        }
        FreeRtos::delay_ms(80);
    }

    for &sq in snake_order.iter().rev() {
        let mut fb = BoardFeedback::new();
        fb.set(sq, SquareFeedback::Capture);
        if let Err(e) = display.show(&fb) {
            log::warn!("LED sweep {sq} failed: {e}");
        }
        FreeRtos::delay_ms(80);
    }

    if let Err(e) = display.show(&BoardFeedback::new()) {
        log::warn!("LED sweep clear failed: {e}");
    }
    FreeRtos::delay_ms(300);

    log::info!("LED sweep complete");
}

/// Continuously read sensors and display results as LED colors.
/// White = white piece, purple = black piece, off = empty.
#[cfg(target_os = "espidf")]
fn sensor_visualization(sensor: &mut Esp32PieceSensor, display: &mut Esp32LedDisplay) {
    use esp_idf_svc::hal::delay::FreeRtos;
    use unnamed_chess_project::feedback::{BoardFeedback, SquareFeedback};
    use unnamed_chess_project::{BoardDisplay, PieceSensor};

    loop {
        let positions = match sensor.read_positions() {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Sensor read failed: {e}");
                FreeRtos::delay_ms(100);
                continue;
            }
        };

        let mut fb = BoardFeedback::new();
        for sq in positions.white {
            fb.set(sq, SquareFeedback::Destination);
        }
        for sq in positions.black {
            fb.set(sq, SquareFeedback::Capture);
        }

        if let Err(e) = display.show(&fb) {
            log::warn!("LED update failed: {e}");
        }

        FreeRtos::delay_ms(50);
    }
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    eprintln!("diagnostics binary is ESP32-only; nothing to do on host");
}
