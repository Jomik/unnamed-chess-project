#[cfg(target_os = "espidf")]
fn main() {
    use esp_idf_svc::eventloop::EspSystemEventLoop;
    use esp_idf_svc::hal::adc::oneshot::AdcDriver;
    use esp_idf_svc::hal::delay::FreeRtos;
    use esp_idf_svc::hal::peripherals::Peripherals;
    use esp_idf_svc::nvs::{
        EspDefaultNvsPartition, EspNvs, EspNvsPartition, NvsCustom, NvsDefault,
    };
    use unnamed_chess_project::esp32::config::{LedPalette, SensorCalibration, SensorConfig};
    use unnamed_chess_project::esp32::{Esp32LedDisplay, Esp32PieceSensor, WifiConnection};
    use unnamed_chess_project::feedback::{BoardFeedback, StatusKind};
    use unnamed_chess_project::player::EmbeddedEngine;
    use unnamed_chess_project::provisioning::BoardConfig;
    use unnamed_chess_project::session::GameSession;
    use unnamed_chess_project::setup::setup_feedback;
    use unnamed_chess_project::{BoardDisplay, PieceSensor};

    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().expect("failed to take peripherals");
    let sys_loop = EspSystemEventLoop::take().expect("failed to take event loop");
    let nvs_partition = EspDefaultNvsPartition::take().expect("failed to take NVS partition");

    let mut display = Esp32LedDisplay::new(peripherals.pins.gpio2, LedPalette::default())
        .expect("failed to init LED display");

    // Load config from NVS
    let nvs = EspNvs::<NvsDefault>::new(nvs_partition.clone(), "config", true)
        .expect("failed to open NVS namespace");

    let config = match BoardConfig::load(&nvs) {
        Ok(Some(config)) => config,
        Ok(None) => {
            log::info!("No config in NVS — entering provisioning mode");
            unnamed_chess_project::esp32::provisioning::run_provisioning_server(
                &mut display,
                peripherals.modem,
                sys_loop,
                nvs_partition,
                nvs,
            );
        }
        Err(e) => {
            log::warn!("NVS read error: {e} — entering provisioning mode");
            unnamed_chess_project::esp32::provisioning::run_provisioning_server(
                &mut display,
                peripherals.modem,
                sys_loop,
                nvs_partition,
                nvs,
            );
        }
    };

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
            log::warn!("NVS calibration read failed: {e} — using defaults");
            SensorConfig::default()
        }
    };

    // Drop NVS handle — partition clone is still available for WiFi
    drop(nvs);

    // Normal boot: connect WiFi
    if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Pending)) {
        log::warn!("LED update failed: {e}");
    }
    let _wifi = match WifiConnection::connect(
        peripherals.modem,
        sys_loop,
        nvs_partition,
        &config.wifi_ssid,
        &config.wifi_pass,
    ) {
        Ok(wifi) => {
            log::info!("WiFi connected");
            if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Success)) {
                log::warn!("LED update failed: {e}");
            }
            FreeRtos::delay_ms(500);
            Some(wifi)
        }
        Err(e) => {
            log::warn!("WiFi failed: {e} — continuing without network");
            if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Failure)) {
                log::warn!("LED update failed: {e}");
            }
            FreeRtos::delay_ms(500);
            None
        }
    };
    if let Err(e) = display.show(&BoardFeedback::default()) {
        log::warn!("LED update failed: {e}");
    }

    // Init sensor
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

    // Wait for starting position
    log::info!("Waiting for starting position...");
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
            }
            None => break,
        }
        FreeRtos::delay_ms(50);
    }
    log::info!("Starting position detected");
    if let Err(e) = display.show(&BoardFeedback::default()) {
        log::warn!("LED clear failed: {e}");
    }

    // Choose opponent
    let opponent: Box<dyn unnamed_chess_project::player::Player> = match config.lichess_token {
        Some(token) if _wifi.is_some() => {
            use unnamed_chess_project::esp32::Esp32LichessClient;
            use unnamed_chess_project::lichess::{LichessConfig, spawn_lichess_opponent};

            let lichess_config = LichessConfig {
                level: config.lichess_level,
                clock_limit: 10800,
                clock_increment: 180,
            };

            let client = Esp32LichessClient::new(token);

            let spawn_fn = |f: Box<dyn FnOnce() + Send>| -> Result<(), String> {
                std::thread::Builder::new()
                    .stack_size(8192)
                    .spawn(f)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            };

            match spawn_lichess_opponent(client, lichess_config, spawn_fn) {
                Ok(lichess_opponent) => {
                    log::info!("Lichess opponent ready");
                    if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Success)) {
                        log::warn!("LED update failed: {e}");
                    }
                    FreeRtos::delay_ms(500);
                    Box::new(lichess_opponent)
                }
                Err(e) => {
                    log::warn!("Lichess setup failed: {e} — falling back to embedded AI");
                    if let Err(e) = display.show(&BoardFeedback::with_status(StatusKind::Failure)) {
                        log::warn!("LED update failed: {e}");
                    }
                    FreeRtos::delay_ms(500);
                    Box::new(EmbeddedEngine::new(unsafe {
                        esp_idf_svc::sys::esp_random()
                    }))
                }
            }
        }
        _ => {
            log::info!("No Lichess token — using embedded AI");
            Box::new(EmbeddedEngine::new(unsafe {
                esp_idf_svc::sys::esp_random()
            }))
        }
    };

    // Game loop (unchanged)
    use unnamed_chess_project::player::HumanPlayer;

    let initial_positions = match sensor.read_positions() {
        Ok(p) => p,
        Err(e) => {
            log::error!("Initial sensor read failed: {e}");
            return;
        }
    };
    let mut session = GameSession::new(Box::new(HumanPlayer::new(initial_positions)), opponent);
    let mut prev = initial_positions;
    log::info!("Game loop started");

    loop {
        let positions = match sensor.read_positions() {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Sensor read failed: {e}");
                FreeRtos::delay_ms(100);
                continue;
            }
        };

        let white_added = positions.white & !prev.white;
        let white_removed = prev.white & !positions.white;
        let black_added = positions.black & !prev.black;
        let black_removed = prev.black & !positions.black;

        for sq in white_added {
            log::debug!("+ {sq} white");
        }
        for sq in white_removed {
            log::debug!("- {sq} white");
        }
        for sq in black_added {
            log::debug!("+ {sq} black");
        }
        for sq in black_removed {
            log::debug!("- {sq} black");
        }
        prev = positions;

        let result = session.tick(positions);

        if let Some(mv) = &result.last_move {
            log::info!("Move played: {mv}");
        }

        if let Err(e) = display.show(&result.feedback) {
            log::warn!("LED update failed: {e}");
        }

        FreeRtos::delay_ms(50);
    }
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    eprintln!(
        "This binary targets ESP32. Use `just flash` to run on hardware, or `just test` to run tests."
    );
}
