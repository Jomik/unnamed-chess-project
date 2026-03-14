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
#[derive(Debug, Clone)]
pub struct SensorConfig {
    /// Resting ADC output (mV) with no magnet. DRV5055A3 is ratiometric at VCC/2.
    pub baseline_mv: u16,
    /// Minimum deviation from baseline (mV) to register a piece.
    pub threshold_mv: u16,
    /// Delay (ms) after switching mux address lines to let the analog signal settle.
    pub settle_delay_ms: u32,
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
        }
    }
}
