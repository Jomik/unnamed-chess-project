# Sensor Diagnostics & Calibration Design

## Problem

The chess board has intermittent LED feedback flicker where entire move guidance patterns shift for a single frame. Logic analyzer captures show clean data on the WS2812 wire, and the fact that full squares (both LEDs of a pair) shift together points to the sensor/feedback layer rather than the RMT driver.

The current `read_positions()` discards raw ADC millivolt values after thresholding, making it impossible to identify noisy or borderline sensor readings. The diagnostics binary only visualizes detected pieces via LEDs — no serial output of raw sensor data.

Additionally, the sensor configuration (baseline 1440mV, threshold 200mV) is hardcoded. Different boards may have slightly different Hall-effect sensor characteristics, so calibration per board improves accuracy.

## Goals

1. **Assembly verification** — pass/fail check after PCB assembly that all sensors, LEDs, and mux addressing work correctly.
2. **Sensor calibration** — derive optimal baseline and threshold values from actual sensor readings, persist to NVS for production use.
3. **Issue diagnosis** — change-based raw ADC logging to identify noisy or borderline squares that could cause feedback flicker.

## Design

### Data Layer: `RawScan` and `read_raw()`

Add a `RawScan` struct and `read_raw()` method to `Esp32PieceSensor` (not the `PieceSensor` trait):

```rust
#[derive(Debug, Clone)]
pub struct RawScan {
    /// Raw millivolt readings indexed by Square as usize.
    /// shakmaty::Square is repr(u8) with A1=0, B1=1, ..., H8=63.
    pub mv: [u16; 64],
}

impl RawScan {
    pub fn deviation(&self, sq: Square, baseline_mv: u16) -> i32 {
        self.mv[sq as usize] as i32 - baseline_mv as i32
    }
}
```

`read_raw()` performs the same mux scan loop as `read_positions()` but stores raw millivolt values per square instead of thresholding them.

Refactor `read_positions()` to call `read_raw()` internally and apply thresholding to the result. This eliminates scan loop duplication while preserving identical behavior.

Export `RawScan` from `esp32/mod.rs`.

### Calibration Data: `SensorCalibration`

Add a calibration struct to `esp32/config.rs`:

```rust
#[derive(Debug, Clone, Copy)]
pub struct SensorCalibration {
    pub baseline_mv: u16,
    pub threshold_mv: u16,
}
```

Both `SensorConfig` and `SensorCalibration` derive `Copy` since they contain only primitive fields.

Add NVS `load`/`save` methods following the existing `BoardConfig` pattern in `esp32/provisioning.rs`. Store both `u16` fields using `get_u16`/`set_u16` (available in `esp-idf-svc` via the ESP-IDF NVS API), with keys `"cal_baseline"` and `"cal_threshold"`.

Calibration data lives in a **separate NVS partition** (`cal`) from the main config partition (`nvs`). This ensures `just erase-nvs` (which erases the `nvs` partition) does not wipe calibration data. Calibration is board-specific hardware data with a different lifecycle from user config — it should survive reprovisioning. Add the `cal` partition to `partitions.csv`.

`SensorCalibration::load`/`save` take an `EspNvsPartition<NvsCustom>` (obtained via `EspNvsPartition::<NvsCustom>::take("cal")`) and open their own `EspNvs<NvsCustom>` handle internally, rather than sharing the `"config"` handle used by `BoardConfig`.

The `cal` partition should be at least `0x3000` (12KB, three 4KB sectors — the minimum for ESP-IDF NVS). `0x4000` (16KB) provides an extra page for wear leveling.

The production firmware loads calibration from NVS at startup. If no calibration data exists, fall back to the current hardcoded defaults (`SensorConfig::default()`).

Add a `Default` impl for `SensorConfig` with values `baseline_mv: 1440`, `threshold_mv: 100`, `settle_delay_ms: 2`. The threshold default was lowered from the original hardcoded 200mV based on hardware testing — magnets produce ~200mV when well-centered, making 200mV too aggressive for uncalibrated boards.

Settle delay is not calibrated — it remains at the default as it depends on hardware timing, not sensor characteristics.

### Diagnostics Binary Pipeline

The diagnostics binary (`src/bin/diagnostics.rs`) runs three phases sequentially:

#### Phase 1: Assembly Check

Starts with empty board, then starting position. Interactive with LED + serial feedback.

**Step 1 — LED sweep** (existing): Light each square sequentially to verify LED wiring.

**Step 2 — Empty board scan (baseline + noise floor)**: Continuous scan loop. Prompts the user to clear all pieces from the board.

```
Clear the board (remove all pieces).
Lit squares = still detecting something. Remove pieces until all LEDs turn off.
```

Each scan cycle, reads all 64 squares via `read_raw()`. Squares that read more than a generous initial margin (e.g., deviation > 100mV from the running average of all 64 readings) are lit on the LEDs, signaling the user to remove any remaining pieces. Once all 64 squares are within the margin, a settling period of 10 consecutive passing scans (~500ms) is required before committing — this prevents transient readings from pieces still being lifted from affecting the baseline.

Once the board is clear:
- **Baseline**: average of all 64 readings.
- **Noise floor**: maximum absolute deviation from baseline among all 64 squares.
- Log: `Baseline: 1435mV, noise floor: 18mV`

This also verifies all 64 mux channels (including ranks 3-6) are alive — a dead or stuck channel would show as an outlier.

**Step 3 — Starting position scan (piece detection)**: Prompts the user to place all pieces in starting position.

```
Place pieces in starting position.
Lit squares = not yet detected. Adjust pieces until all LEDs turn off.
```

Each scan cycle, reads all 64 squares. Using the baseline and noise floor from Step 2, a square passes if:
- **Occupied squares** (ranks 1-2, 7-8): absolute deviation from baseline significantly exceeds the noise floor (e.g., >3x), with correct color polarity (ranks 1-2 above baseline = white, ranks 7-8 below = black).
- **Empty squares** (ranks 3-6): absolute deviation stays below the noise floor threshold (no pieces accidentally placed).

Light up failing squares. Log remaining failures periodically (every ~5s): `Waiting: b2 (+148mV weak), g8 (not detected) — 2 squares remaining`

Once all 64 squares pass, a settling period of 10 consecutive passing scans (~500ms) is required before committing — this ensures pieces are stable. If any square regresses during settling, the counter resets. If a sensor is dead, the board needs hardware repair — the loop continues indefinitely until all squares are fixed or the user resets the board.

Log final per-square readings to serial. The baseline, noise floor, and weakest piece signal from Steps 2-3 feed directly into calibration (Phase 2).


#### Phase 2: Calibration

No additional user action needed — uses values from Steps 2 and 3.

- **Baseline**: From Step 2 (empty board average).
- **Threshold**: Midpoint between the noise floor (Step 2) and the weakest occupied-square deviation (Step 3). This maximizes the margin on both sides.

Prints the calibrated values and saves to NVS:

```
=== Calibration ===
  Baseline: 1435mV (64-square empty board average)
  Noise floor: 18mV
  Weakest piece: +198mV
  Calibrated threshold: 108mV (midpoint of 18 and 198)
  Saved to NVS.
```

#### Phase 3: Sensor Diagnosis (infinite loop)

Change-based logging that stays quiet when the board is stable and becomes verbose when something changes.

**Initial snapshot**: Full 8x8 grid of signed deviations using calibrated baseline, printed once at start.

```
=== Sensor Diagnosis (baseline=1435mV, threshold=118mV) ===
     a     b     c     d     e     f     g     h
8: -223  -218  -210  -215  -230  -208  -212  -220
7: -205  -198  -215  -220  -210  -208  -215  -212
6:  +12   -08   +05   +03   -11   +07   +02   -06
5:  +07   +11   -03   +05   +08   -06   +12   +04
4:  +03   -08   +10   -04   +06   +09   -07   +05
3:  +11   +03   -09   +07   +04   -08   +06   +10
2: +215  +198  +215  +220  +210  +208  +215  +212
1: +218  +220  +215  +210  +225  +208  +212  +219
```

**Change-based logging**: Each scan compares readings against the previous scan. Only log squares where the reading changed by more than a significance threshold (default 50mV, defined as a const for easy tuning).

```
[scan 042] (stable 2.1s)
[scan 043] CHANGED b2: +198 -> +145 (53mV drop)
[scan 044] CHANGED b2: +145 -> +205 (recovered)
```

**Periodic summary** (every ~5 seconds): Lists squares that changed during the interval, with min/max deviation range. Identifies flicker candidates.

```
--- summary (scans 40-140, 5.0s) ---
  noisy: b2 (range +145..+212, 3 changes)
  stable: 63/64 squares
```

### Production Firmware Change

In `main.rs`, load `SensorCalibration` from NVS before creating `Esp32PieceSensor`. If present, use the calibrated values; otherwise fall back to `SensorConfig::default()`.

```rust
// Load calibration from the separate "cal" NVS partition
let cal_partition = EspNvsPartition::<NvsCustom>::take("cal")
    .expect("failed to take cal NVS partition");
let sensor_config = match SensorCalibration::load(&cal_partition) {
    Ok(Some(cal)) => {
        log::info!("Using NVS calibration: baseline={}mV, threshold={}mV",
            cal.baseline_mv, cal.threshold_mv);
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
```

Calibration is loaded from the separate `cal` NVS partition (via `EspNvsPartition::<NvsCustom>::take("cal")`), independent of the `nvs` partition used by `BoardConfig`. `just erase-nvs` erases the `nvs` partition by name, leaving `cal` untouched. Add a separate `just erase-cal` recipe for forcing recalibration after hardware changes.

## Files Modified

| File | Change |
|------|--------|
| `src/esp32/sensor.rs` | Add `RawScan`, `read_raw()`, refactor `read_positions()` |
| `src/esp32/mod.rs` | Export `RawScan` |
| `src/esp32/config.rs` | Add `SensorCalibration` struct, `Default` for `SensorConfig` |
| `src/esp32/provisioning.rs` | Add NVS `load`/`save` for `SensorCalibration` (on `cal` partition) |
| `partitions.csv` | Add `cal` NVS partition for calibration data |
| `src/bin/diagnostics.rs` | Assembly check, calibration, change-based diagnosis |
| `src/main.rs` | Load calibration from NVS with fallback to defaults |

## Verification

1. `just test` — existing tests pass (refactored `read_positions()` has identical behavior).
2. `just clippy` — no warnings.
3. `just flash-diag` — start with empty board (LED sweep + baseline scan), then place starting position (piece detection + calibration saved to NVS).
4. `just flash` — production firmware loads calibrated values from NVS, falls back to defaults if uncalibrated.
5. During diagnosis mode: leave board still and observe output is quiet; move a piece and observe change events logged.
