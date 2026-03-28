#[cfg(target_os = "espidf")]
use esp_idf_svc::hal::delay::FreeRtos;
#[cfg(target_os = "espidf")]
use shakmaty::{File, Rank, Square};
#[cfg(target_os = "espidf")]
use unnamed_chess_project::BoardDisplay;
#[cfg(target_os = "espidf")]
use unnamed_chess_project::feedback::{BoardFeedback, SquareFeedback};

#[cfg(target_os = "espidf")]
enum SquareCheck {
    Pass {
        abs_dev: u16,
    },
    Fail {
        feedback: SquareFeedback,
        reason: &'static str,
    },
}

#[cfg(target_os = "espidf")]
fn check_square(
    dev: i32,
    abs_dev: u16,
    is_occupied: bool,
    rank: u32,
    piece_threshold: u16,
) -> SquareCheck {
    if is_occupied {
        let correct_polarity = if rank <= 1 { dev > 0 } else { dev < 0 };
        if abs_dev < piece_threshold {
            SquareCheck::Fail {
                feedback: SquareFeedback::Destination,
                reason: "weak",
            }
        } else if !correct_polarity {
            SquareCheck::Fail {
                feedback: SquareFeedback::Capture,
                reason: "wrong color",
            }
        } else {
            SquareCheck::Pass { abs_dev }
        }
    } else if abs_dev > piece_threshold {
        SquareCheck::Fail {
            feedback: SquareFeedback::Capture,
            reason: "unexpected piece",
        }
    } else {
        SquareCheck::Pass { abs_dev }
    }
}

#[cfg(target_os = "espidf")]
fn log_summary(
    summary_scan_start: u32,
    scan_count: u32,
    min_dev: &[i32; 64],
    max_dev: &[i32; 64],
    change_count: &[u32; 64],
) {
    let noisy: Vec<String> = (0..64u32)
        .filter(|&i| change_count[i as usize] > 0)
        .map(|i| {
            let sq = Square::new(i);
            let lo = min_dev[i as usize];
            let hi = max_dev[i as usize];
            let n = change_count[i as usize];
            format!("{sq} (range {lo:+}..{hi:+}, {n} changes)")
        })
        .collect();

    let stable_count = change_count.iter().filter(|&&c| c == 0).count();
    log::info!(
        "--- summary (scans {}..{}) ---",
        summary_scan_start,
        scan_count
    );
    if noisy.is_empty() {
        log::info!("  all 64 squares stable");
    } else {
        for entry in &noisy {
            log::info!("  noisy: {entry}");
        }
        log::info!("  stable: {stable_count}/64 squares");
    }
}

/// Light one square at a time in a snake pattern (a1→h1, h2→a2, …) and back,
/// to verify LED wiring without drawing excessive current.
#[cfg(target_os = "espidf")]
fn led_sweep(display: &mut unnamed_chess_project::esp32::Esp32LedDisplay) {
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

/// Phase 1, Step 2: Empty board scan for baseline and noise floor.
///
/// Prompts user to clear the board. Lights up squares that still detect
/// something. Returns (baseline_mv, noise_floor_mv) once all squares are clear
/// for SETTLE_SCANS consecutive scans.
#[cfg(target_os = "espidf")]
fn empty_board_scan(
    sensor: &mut unnamed_chess_project::esp32::Esp32PieceSensor,
    display: &mut unnamed_chess_project::esp32::Esp32LedDisplay,
) -> (u16, u16) {
    log::info!("Clear the board (remove all pieces).");
    log::info!("Lit squares = still detecting something.");

    const EMPTY_MARGIN_MV: u16 = 100;
    const SETTLE_SCANS: u32 = 10;

    let mut consecutive_clear: u32 = 0;

    loop {
        let scan = match sensor.read_raw() {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Scan failed: {e}");
                FreeRtos::delay_ms(100);
                continue;
            }
        };

        // Compute mean across all 64 squares
        let avg: u16 = {
            let sum: u32 = scan.mv.iter().map(|&v| v as u32).sum();
            (sum / 64) as u16
        };

        // Find squares that deviate too much from average
        let mut fb = BoardFeedback::new();
        let mut all_clear = true;
        for sq_idx in 0..64u32 {
            let sq = Square::new(sq_idx);
            let dev = (scan.mv[sq_idx as usize] as i32 - avg as i32).unsigned_abs() as u16;
            if dev > EMPTY_MARGIN_MV {
                fb.set(sq, SquareFeedback::Capture);
                all_clear = false;
            }
        }

        if let Err(e) = display.show(&fb) {
            log::warn!("LED update failed: {e}");
        }

        if !all_clear {
            consecutive_clear = 0;
            FreeRtos::delay_ms(50);
            continue;
        }

        if consecutive_clear == 0 {
            log::info!("Board clear, settling...");
        }
        consecutive_clear += 1;

        if consecutive_clear < SETTLE_SCANS {
            FreeRtos::delay_ms(50);
            continue;
        }

        // Compute final baseline and noise floor from this last scan
        let baseline = avg;
        let noise_floor = scan
            .mv
            .iter()
            .map(|&v| (v as i32 - baseline as i32).unsigned_abs() as u16)
            .max()
            .unwrap_or(0);

        log::info!("Board clear.");
        log::info!("Baseline: {baseline}mV, noise floor: {noise_floor}mV");
        return (baseline, noise_floor);
    }
}

/// Phase 1, Step 3: Starting position scan for piece detection verification.
///
/// Prompts user to place starting position. Lights up squares that fail.
/// Returns the minimum absolute deviation among occupied squares (weakest piece signal).
#[cfg(target_os = "espidf")]
fn starting_position_scan(
    sensor: &mut unnamed_chess_project::esp32::Esp32PieceSensor,
    display: &mut unnamed_chess_project::esp32::Esp32LedDisplay,
    baseline: u16,
    noise_floor: u16,
) -> u16 {
    log::info!("Place pieces in starting position.");
    log::info!("Lit squares = not yet detected. Adjust pieces until all LEDs turn off.");

    let piece_threshold = noise_floor.saturating_mul(3).max(10);
    let mut last_log = std::time::Instant::now();
    const SETTLE_SCANS: u32 = 10;
    let mut consecutive_pass: u32 = 0;

    loop {
        let scan = match sensor.read_raw() {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Scan failed: {e}");
                FreeRtos::delay_ms(100);
                continue;
            }
        };

        let mut fb = BoardFeedback::new();
        let mut all_pass = true;
        let mut weakest_piece: u16 = u16::MAX;
        let mut failing_squares: Vec<(Square, &str)> = Vec::new();

        for sq_idx in 0..64u32 {
            let sq = Square::new(sq_idx);
            let rank = sq.rank() as u32;
            let dev = scan.deviation(sq, baseline);
            let abs_dev = dev.unsigned_abs() as u16;
            let is_occupied = rank <= 1 || rank >= 6;

            match check_square(dev, abs_dev, is_occupied, rank, piece_threshold) {
                SquareCheck::Pass { abs_dev } if is_occupied => {
                    weakest_piece = weakest_piece.min(abs_dev);
                }
                SquareCheck::Pass { .. } => {}
                SquareCheck::Fail { feedback, reason } => {
                    fb.set(sq, feedback);
                    failing_squares.push((sq, reason));
                    all_pass = false;
                }
            }
        }

        if let Err(e) = display.show(&fb) {
            log::warn!("LED update failed: {e}");
        }

        // Periodic failure logging
        if !all_pass && last_log.elapsed().as_secs() >= 5 {
            let summary: Vec<String> = failing_squares
                .iter()
                .take(5)
                .map(|(sq, reason)| {
                    let dev = scan.deviation(*sq, baseline);
                    format!("{sq} ({reason}, {dev:+}mV)")
                })
                .collect();
            let remaining = failing_squares.len();
            log::info!(
                "Waiting: {} — {remaining} squares remaining",
                summary.join(", ")
            );
            last_log = std::time::Instant::now();
        }

        if !all_pass {
            consecutive_pass = 0;
            FreeRtos::delay_ms(50);
            continue;
        }

        if consecutive_pass == 0 {
            log::info!("All pieces detected, settling...");
        }
        consecutive_pass += 1;

        if consecutive_pass < SETTLE_SCANS {
            FreeRtos::delay_ms(50);
            continue;
        }

        log::info!("All pieces detected.");
        log_deviation_grid(&scan, baseline);
        return weakest_piece;
    }
}

/// Phase 3: Change-based sensor diagnosis loop.
///
/// Logs an initial 8x8 grid, then only logs squares whose readings change
/// by more than `CHANGE_THRESHOLD_MV` between consecutive scans.
/// Prints a periodic summary every ~5 seconds.
#[cfg(target_os = "espidf")]
fn diagnosis_loop(
    sensor: &mut unnamed_chess_project::esp32::Esp32PieceSensor,
    baseline: u16,
    threshold: u16,
) -> ! {
    const CHANGE_THRESHOLD_MV: u16 = 50;
    const SUMMARY_INTERVAL_SCANS: u32 = 100; // ~5s at 50ms per scan

    log::info!("=== Sensor Diagnosis (baseline={baseline}mV, threshold={threshold}mV) ===");

    // Initial snapshot
    let mut prev_scan = match sensor.read_raw() {
        Ok(s) => s,
        Err(e) => {
            log::error!("Initial scan failed: {e}");
            loop {
                FreeRtos::delay_ms(1000);
            }
        }
    };

    // Print initial grid
    log_deviation_grid(&prev_scan, baseline);

    let mut scan_count: u32 = 0;
    let mut last_change_time = std::time::Instant::now();

    // Per-square tracking for periodic summary
    let mut min_dev = [i32::MAX; 64];
    let mut max_dev = [i32::MIN; 64];
    let mut change_count = [0u32; 64];
    let mut summary_scan_start: u32 = 0;

    loop {
        FreeRtos::delay_ms(50);
        scan_count += 1;

        let scan = match sensor.read_raw() {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Scan {scan_count} failed: {e}");
                continue;
            }
        };

        // Check for changes
        let mut any_change = false;
        for sq_idx in 0..64u32 {
            let sq = Square::new(sq_idx);
            let prev_mv = prev_scan.mv[sq_idx as usize];
            let curr_mv = scan.mv[sq_idx as usize];
            let diff = (curr_mv as i32 - prev_mv as i32).unsigned_abs() as u16;

            let dev = scan.deviation(sq, baseline);
            min_dev[sq_idx as usize] = min_dev[sq_idx as usize].min(dev);
            max_dev[sq_idx as usize] = max_dev[sq_idx as usize].max(dev);

            if diff > CHANGE_THRESHOLD_MV {
                let prev_dev = prev_scan.deviation(sq, baseline);
                log::info!(
                    "[scan {scan_count:03}] CHANGED {sq}: {prev_dev:+} -> {dev:+} ({diff}mV)",
                );
                change_count[sq_idx as usize] += 1;
                any_change = true;
            }
        }

        if any_change {
            last_change_time = std::time::Instant::now();
        }
        if !any_change && scan_count.is_multiple_of(SUMMARY_INTERVAL_SCANS) {
            let stable_secs = last_change_time.elapsed().as_secs_f32();
            log::info!("[scan {scan_count:03}] (stable {stable_secs:.1}s)");
        }

        // Periodic summary
        if scan_count.is_multiple_of(SUMMARY_INTERVAL_SCANS) {
            log_summary(
                summary_scan_start,
                scan_count,
                &min_dev,
                &max_dev,
                &change_count,
            );
            min_dev = [i32::MAX; 64];
            max_dev = [i32::MIN; 64];
            change_count = [0u32; 64];
            summary_scan_start = scan_count;
        }

        prev_scan = scan;
    }
}

/// Log an 8x8 grid of per-square deviations from baseline.
#[cfg(target_os = "espidf")]
fn log_deviation_grid(scan: &unnamed_chess_project::esp32::RawScan, baseline: u16) {
    log::info!("     a     b     c     d     e     f     g     h");
    for rank_idx in (0..8u32).rev() {
        let rank = Rank::new(rank_idx);
        let mut row = format!("{}:", rank_idx + 1);
        for file_idx in 0..8u32 {
            let sq = Square::from_coords(File::new(file_idx), rank);
            let dev = scan.deviation(sq, baseline);
            row.push_str(&format!(" {dev:+5}"));
        }
        log::info!("{row}");
    }
}

#[cfg(target_os = "espidf")]
fn main() {
    use esp_idf_svc::hal::adc::oneshot::AdcDriver;
    use esp_idf_svc::hal::peripherals::Peripherals;
    use esp_idf_svc::nvs::{EspNvsPartition, NvsCustom};
    use unnamed_chess_project::esp32::config::{LedPalette, Rgb8, SensorCalibration, SensorConfig};
    use unnamed_chess_project::esp32::{Esp32LedDisplay, Esp32PieceSensor};

    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().expect("failed to take peripherals");

    let adc_driver = AdcDriver::new(peripherals.adc1).expect("failed to init ADC1");

    let sensor_config = SensorConfig::default();
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

    let diag_palette = LedPalette {
        off: Rgb8::new(0, 0, 0),
        destination: Rgb8::new(15, 15, 15),
        capture: Rgb8::new(15, 0, 15),
        origin: Rgb8::new(0, 0, 0),
        check: Rgb8::new(0, 0, 0),
        checker: Rgb8::new(0, 0, 0),
        victory: Rgb8::new(0, 0, 0),
        stalemate: Rgb8::new(0, 0, 0),
        status_pending: Rgb8::new(0, 0, 0),
        status_success: Rgb8::new(0, 0, 0),
        status_failure: Rgb8::new(0, 0, 0),
    };

    let mut display = Esp32LedDisplay::new(peripherals.pins.gpio2, diag_palette)
        .expect("failed to init LED display");

    log::info!("=== Phase 1: Assembly Check ===");

    log::info!("--- Step 1: LED sweep ---");
    led_sweep(&mut display);

    log::info!("--- Step 2: Empty board scan ---");
    let (baseline, noise_floor) = empty_board_scan(&mut sensor, &mut display);

    log::info!("--- Step 3: Starting position scan ---");
    let weakest_piece = starting_position_scan(&mut sensor, &mut display, baseline, noise_floor);
    log::info!("Weakest piece signal: {weakest_piece}mV deviation from baseline");

    log::info!("=== Phase 2: Calibration ===");
    let threshold = ((noise_floor as u32 + weakest_piece as u32) / 2) as u16;
    if noise_floor >= weakest_piece {
        log::warn!(
            "  Noise floor ({noise_floor}mV) >= weakest piece signal ({weakest_piece}mV) — signal-to-noise ratio is insufficient for reliable detection. Check sensor placement."
        );
    }
    log::info!("  Baseline: {baseline}mV (64-square empty board average)");
    log::info!("  Noise floor: {noise_floor}mV");
    log::info!("  Weakest piece: +{weakest_piece}mV");
    log::info!(
        "  Calibrated threshold: {threshold}mV (midpoint of {noise_floor} and {weakest_piece})"
    );

    // Save to the dedicated cal partition (survives erase-nvs)
    let cal_partition =
        EspNvsPartition::<NvsCustom>::take("cal").expect("failed to take cal NVS partition");
    let cal = SensorCalibration {
        baseline_mv: baseline,
        threshold_mv: threshold,
    };
    match cal.save(&cal_partition) {
        Ok(()) => log::info!("  Saved to NVS."),
        Err(e) => log::warn!("  NVS save failed: {e}"),
    }

    log::info!("=== Phase 3: Sensor Diagnosis ===");
    diagnosis_loop(&mut sensor, baseline, threshold);
}

#[cfg(not(target_os = "espidf"))]
fn main() {
    eprintln!("diagnostics binary is ESP32-only; nothing to do on host");
}
