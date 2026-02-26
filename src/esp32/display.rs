use crate::BoardDisplay;
use crate::feedback::BoardFeedback;

/// Error types for ESP32 LED display operations.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LedDisplayError {
    #[error("LED driver initialization failed: {0}")]
    DriverInit(String),
    #[error("LED update error")]
    UpdateError,
}

/// WS2812 LED array driven via the ESP32 RMT peripheral.
///
/// Maps [`SquareFeedback`](crate::feedback::SquareFeedback) variants
/// to LED colors for the 8×8 grid beneath the board.
#[derive(Debug)]
pub struct Esp32LedDisplay {
    // TODO: Add RMT/SPI peripheral and LED buffer
}

impl Esp32LedDisplay {
    pub fn new() -> Result<Self, LedDisplayError> {
        todo!("Initialize WS2812 LED driver via RMT peripheral")
    }
}

impl BoardDisplay for Esp32LedDisplay {
    type Error = LedDisplayError;

    fn show(&mut self, _feedback: &BoardFeedback) -> Result<(), Self::Error> {
        todo!("Map feedback to LED colors and push to WS2812 strip")
    }
}
