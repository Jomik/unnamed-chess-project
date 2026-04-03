/// Simple RGB color for WS2812 LEDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Sensor configuration for ADC thresholds and timing.
#[derive(Debug, Clone, Copy)]
pub struct SensorConfig {
    /// Resting ADC output (mV) with no magnet. DRV5055A3 is ratiometric at VCC/2.
    pub baseline_mv: u16,
    /// Minimum deviation from baseline (mV) to register a piece.
    pub threshold_mv: u16,
    /// Delay (ms) after switching mux address lines to let the analog signal settle.
    pub settle_delay_ms: u32,
}

impl Default for SensorConfig {
    fn default() -> Self {
        Self {
            baseline_mv: 1440,
            threshold_mv: 100,
            settle_delay_ms: 2,
        }
    }
}

/// Per-board sensor calibration derived from the diagnostics binary.
///
/// Stored in NVS and loaded by the production firmware to replace
/// hardcoded defaults. See `docs/specs/2026-03-28-sensor-diagnostics-design.md`.
#[derive(Debug, Clone, Copy)]
pub struct SensorCalibration {
    pub baseline_mv: u16,
    pub threshold_mv: u16,
}

/// Error type for NVS calibration load/save operations.
#[derive(Debug, thiserror::Error)]
#[error("NVS error: {0}")]
pub struct CalibrationError(#[from] pub esp_idf_svc::sys::EspError);

const CAL_NAMESPACE: &str = "cal";
const KEY_CAL_BASELINE: &str = "cal_baseline";
const KEY_CAL_THRESHOLD: &str = "cal_threshold";

#[cfg(target_os = "espidf")]
impl SensorCalibration {
    /// Load calibration from the dedicated `cal` NVS partition.
    ///
    /// Returns `Ok(None)` if no calibration has been saved yet (first boot or
    /// after `just erase-cal`). Returns `Err` on NVS read failures.
    pub fn load(
        partition: &esp_idf_svc::nvs::EspNvsPartition<esp_idf_svc::nvs::NvsCustom>,
    ) -> Result<Option<Self>, CalibrationError> {
        use esp_idf_svc::nvs::EspNvs;

        let nvs = match EspNvs::new(partition.clone(), CAL_NAMESPACE, false) {
            Ok(nvs) => nvs,
            Err(e) if e.code() == esp_idf_svc::sys::ESP_ERR_NVS_NOT_FOUND => return Ok(None),
            Err(e) => return Err(CalibrationError(e)),
        };

        let baseline_mv = match nvs.get_u16(KEY_CAL_BASELINE).map_err(CalibrationError)? {
            Some(v) => v,
            None => return Ok(None),
        };

        let threshold_mv = nvs
            .get_u16(KEY_CAL_THRESHOLD)
            .map_err(CalibrationError)?
            .unwrap_or(SensorConfig::default().threshold_mv);

        Ok(Some(SensorCalibration {
            baseline_mv,
            threshold_mv,
        }))
    }

    /// Save calibration to the dedicated `cal` NVS partition.
    pub fn save(
        &self,
        partition: &esp_idf_svc::nvs::EspNvsPartition<esp_idf_svc::nvs::NvsCustom>,
    ) -> Result<(), CalibrationError> {
        use esp_idf_svc::nvs::EspNvs;

        let nvs = EspNvs::new(partition.clone(), CAL_NAMESPACE, true).map_err(CalibrationError)?;
        nvs.set_u16(KEY_CAL_BASELINE, self.baseline_mv)
            .map_err(CalibrationError)?;
        nvs.set_u16(KEY_CAL_THRESHOLD, self.threshold_mv)
            .map_err(CalibrationError)?;
        Ok(())
    }
}

/// Display configuration for LED colors.
#[derive(Debug, Clone)]
pub struct DisplayConfig {
    pub palette: LedPalette,
}

/// LED colors for each feedback type.
///
/// All values are RGB8 with conservative brightness to avoid
/// washing out through the board surface.
#[derive(Debug, Clone, Copy)]
pub struct LedPalette {
    pub off: Rgb8,
    pub destination: Rgb8,
    pub capture: Rgb8,
    pub origin: Rgb8,
    pub check: Rgb8,
    pub checker: Rgb8,
    pub victory: Rgb8,
    pub stalemate: Rgb8,
    pub status_pending: Rgb8,
    pub status_success: Rgb8,
    pub status_failure: Rgb8,
}

impl Default for LedPalette {
    fn default() -> Self {
        Self {
            off: Rgb8::new(0, 0, 0),
            destination: Rgb8::new(0, 20, 0),
            capture: Rgb8::new(20, 10, 0),
            origin: Rgb8::new(0, 0, 20),
            check: Rgb8::new(20, 0, 0),
            checker: Rgb8::new(20, 0, 0),
            victory: Rgb8::new(0, 20, 0),
            stalemate: Rgb8::new(20, 15, 0),
            status_pending: Rgb8::new(0, 0, 20),
            status_success: Rgb8::new(0, 20, 0),
            status_failure: Rgb8::new(20, 0, 0),
        }
    }
}
