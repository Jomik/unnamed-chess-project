use unnamed_chess_project::hardware;

#[cfg(target_os = "espidf")]
fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Chess Board - ESP32");
    loop {
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(1000);
    }
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    let sensor = hardware::MockPieceSensor::new();
    unnamed_chess_project::visualization::run_interactive_terminal(sensor);
}
