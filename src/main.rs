use unnamed_chess_project::{hardware, visualization};

#[cfg(target_arch = "xtensa")]
fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Chess Board - ESP32");
    loop {
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(1000);
    }
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {
    let sensor = hardware::MockPieceSensor::new();
    visualization::run_interactive_terminal(sensor);
}
