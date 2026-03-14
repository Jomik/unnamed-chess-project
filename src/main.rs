#[cfg(target_os = "espidf")]
fn main() {
    use esp_idf_svc::hal::adc::attenuation;
    use esp_idf_svc::hal::adc::oneshot::config::AdcChannelConfig;
    use esp_idf_svc::hal::adc::oneshot::{AdcChannelDriver, AdcDriver};
    use esp_idf_svc::hal::gpio::PinDriver;
    use esp_idf_svc::hal::peripherals::Peripherals;
    use smart_leds::RGB8;
    use smart_leds::SmartLedsWrite;
    use ws2812_esp32_rmt_driver::Ws2812Esp32Rmt;

    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Taking peripherals...");
    let peripherals = Peripherals::take().expect("Failed to take peripherals");
    let mut ws2812 = Ws2812Esp32Rmt::new(peripherals.rmt.channel0, peripherals.pins.gpio2)
        .expect("Failed to init WS2812");

    let mut s0 = PinDriver::output(peripherals.pins.gpio9).expect("Failed to init S0");
    let mut s1 = PinDriver::output(peripherals.pins.gpio10).expect("Failed to init S1");
    let mut s2 = PinDriver::output(peripherals.pins.gpio11).expect("Failed to init S2");
    let mut s3 = PinDriver::output(peripherals.pins.gpio12).expect("Failed to init S3");

    let adc = AdcDriver::new(peripherals.adc1).expect("Failed to init ADC1");
    let adc_config = AdcChannelConfig {
        attenuation: attenuation::DB_11,
        ..Default::default()
    };
    let mut mux0 = AdcChannelDriver::new(&adc, peripherals.pins.gpio4, &adc_config)
        .expect("Failed to init mux0 ADC (GPIO4)");
    let mut mux1 = AdcChannelDriver::new(&adc, peripherals.pins.gpio5, &adc_config)
        .expect("Failed to init mux1 ADC (GPIO5)");
    let mut mux2 = AdcChannelDriver::new(&adc, peripherals.pins.gpio6, &adc_config)
        .expect("Failed to init mux2 ADC (GPIO6)");
    let mut mux3 = AdcChannelDriver::new(&adc, peripherals.pins.gpio7, &adc_config)
        .expect("Failed to init mux3 ADC (GPIO7)");

    const BASELINE: u16 = 1440;
    const THRESHOLD: u16 = 200;
    const NUM_LEDS: usize = 128;
    const LEDS_PER_ROW: usize = 16;

    fn square_leds(row: usize, col: usize) -> (usize, usize) {
        let base = row * LEDS_PER_ROW;
        (base + col, base + 15 - col)
    }

    log::info!("Sensor + LED test (baseline={BASELINE}, threshold=±{THRESHOLD})");

    let off = RGB8 { r: 0, g: 0, b: 0 };
    let north = RGB8 { r: 0, g: 0, b: 20 };
    let south = RGB8 { r: 20, g: 0, b: 0 };

    loop {
        let mut leds = [off; NUM_LEDS];

        for channel in 0u8..16 {
            if channel & 1 != 0 {
                s0.set_high()
            } else {
                s0.set_low()
            }
            .expect("S0");
            if channel & 2 != 0 {
                s1.set_high()
            } else {
                s1.set_low()
            }
            .expect("S1");
            if channel & 4 != 0 {
                s2.set_high()
            } else {
                s2.set_low()
            }
            .expect("S2");
            if channel & 8 != 0 {
                s3.set_high()
            } else {
                s3.set_low()
            }
            .expect("S3");

            esp_idf_svc::hal::delay::FreeRtos::delay_ms(2);

            let readings = [
                mux0.read().unwrap_or(9999),
                mux1.read().unwrap_or(9999),
                mux2.read().unwrap_or(9999),
                mux3.read().unwrap_or(9999),
            ];

            for (mux_idx, val) in readings.iter().enumerate() {
                if val.abs_diff(BASELINE) > THRESHOLD {
                    let color = if *val > BASELINE { south } else { north };
                    let row = mux_idx * 2 + (channel as usize / 8);
                    let raw_col = (channel as usize) % 8;
                    let col = if row % 2 == 1 { 7 - raw_col } else { raw_col };
                    let (led1, led2) = square_leds(row, col);
                    if led1 < NUM_LEDS {
                        leds[led1] = color;
                    }
                    if led2 < NUM_LEDS {
                        leds[led2] = color;
                    }
                }
            }
        }

        SmartLedsWrite::write(&mut ws2812, leds.iter().copied()).ok();
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(50);
    }
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    unnamed_chess_project::mock::run_interactive_terminal();
}
