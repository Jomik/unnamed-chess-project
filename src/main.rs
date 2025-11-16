mod hardware;

#[cfg(all(feature = "visualization", not(target_arch = "xtensa")))]
mod visualization;

#[cfg(target_arch = "xtensa")]
fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Chess Board - ESP32 Mode");

    // TODO: Initialize real hardware once available

    loop {
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(1000);
    }
}

#[cfg(not(target_arch = "xtensa"))]
fn main() {
    println!("Chess Board - Development Mode");

    #[cfg(feature = "visualization")]
    {
        let mut board = hardware::MockChessBoard::new();
        board.setup_initial_position();

        visualization::run_interactive_terminal(board);
    }

    #[cfg(not(feature = "visualization"))]
    {
        println!("Run with: cargo run --features visualization");
    }
}
