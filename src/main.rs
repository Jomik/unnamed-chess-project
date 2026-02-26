#[cfg(target_os = "espidf")]
fn main() {
    use unnamed_chess_project::BoardDisplay;
    use unnamed_chess_project::esp32::{Esp32LedDisplay, Esp32PieceSensor};
    use unnamed_chess_project::feedback;
    use unnamed_chess_project::game_logic::GameEngine;

    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let mut sensor = Esp32PieceSensor::from().expect("Failed to initialize piece sensor");
    let mut display = Esp32LedDisplay::new().expect("Failed to initialize LED display");
    let mut engine = GameEngine::new();

    log::info!("System initialized, starting main loop");
    loop {
        match sensor.read_positions() {
            Ok(positions) => {
                let state = engine.tick(positions);
                let fb = feedback::compute_feedback(&state);
                if let Err(e) = display.show(&fb) {
                    log::warn!("Display error: {}", e);
                }
            }
            Err(e) => {
                log::warn!("Sensor read error: {}", e);
            }
        }

        esp_idf_svc::hal::delay::FreeRtos::delay_ms(50);
    }
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    unnamed_chess_project::mock::run_interactive_terminal();
}
