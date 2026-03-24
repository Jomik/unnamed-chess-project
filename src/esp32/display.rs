use core::time::Duration;

use esp_idf_svc::hal::gpio::OutputPin;
use esp_idf_svc::hal::rmt::config::{TransmitConfig, TxChannelConfig};
use esp_idf_svc::hal::rmt::encoder::{BytesEncoder, BytesEncoderConfig, RawEncoder};
use esp_idf_svc::hal::rmt::{PinState, Symbol, TxChannelDriver};
use esp_idf_svc::hal::units::FromValueType;

use crate::BoardDisplay;
use crate::esp32::config::{LedPalette, Rgb8};
use crate::feedback::{BoardFeedback, SquareFeedback, StatusKind};

const NUM_LEDS: usize = 128;
const LEDS_PER_ROW: usize = 16;

/// RMT tick resolution for WS2812 timing. 10 MHz gives 100ns per tick,
/// sufficient for the ~300-900ns pulse widths in the WS2812 protocol.
const RMT_RESOLUTION_HZ: u32 = 10_000_000;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LedDisplayError {
    #[error("LED driver initialization failed: {0}")]
    DriverInit(String),
    #[error("LED write failed: {0}")]
    WriteFailed(String),
}

/// WS2812 LED strip driven via the ESP32 RMT peripheral.
///
/// 128 LEDs in a snake pattern: 16 per row (2 per square), 8 rows.
/// Maps [`SquareFeedback`] variants to colors via a configurable [`LedPalette`].
pub struct Esp32LedDisplay<'d> {
    channel: TxChannelDriver<'d>,
    encoder: BytesEncoder,
    buffer: [Rgb8; NUM_LEDS],
    palette: LedPalette,
}

/// Map a board square to its two LED indices in the snake-wired strip.
///
/// Each square has 2 LEDs. Within a row of 16 LEDs, the square at
/// column `c` maps to indices `base + c` and `base + 15 - c`, where
/// `base = row * 16`. This creates paired LEDs that converge toward
/// the center of each row.
fn leds_for_square(sq: shakmaty::Square) -> (usize, usize) {
    let row = sq.rank() as usize;
    let col = sq.file() as usize;
    let base = row * LEDS_PER_ROW;
    (base + col, base + 15 - col)
}

/// Build WS2812 bit symbols for the RMT encoder.
///
/// WS2812 protocol: each bit is a high pulse followed by a low pulse.
/// - Bit 0: ~400ns high, ~850ns low
/// - Bit 1: ~800ns high, ~450ns low
fn ws2812_bytes_encoder_config() -> Result<BytesEncoderConfig, LedDisplayError> {
    let resolution = RMT_RESOLUTION_HZ.Hz();
    let bit0 = Symbol::new_with(
        resolution,
        PinState::High,
        Duration::from_nanos(400),
        PinState::Low,
        Duration::from_nanos(850),
    )
    .map_err(|e| LedDisplayError::DriverInit(format!("bit0 symbol: {e}")))?;

    let bit1 = Symbol::new_with(
        resolution,
        PinState::High,
        Duration::from_nanos(800),
        PinState::Low,
        Duration::from_nanos(450),
    )
    .map_err(|e| LedDisplayError::DriverInit(format!("bit1 symbol: {e}")))?;

    Ok(BytesEncoderConfig {
        bit0,
        bit1,
        msb_first: true,
        ..Default::default()
    })
}

/// Workaround for esp-idf-hal `send_and_wait` double-wrapping `RawEncoder`
/// types in `EncoderWrapper`. Replace with `channel.send_and_wait()` once fixed.
/// https://github.com/esp-rs/esp-idf-hal/issues/575
///
/// # Safety
/// `start_send` is unsafe because the hardware accesses encoder/data
/// asynchronously. `wait_all_done` blocks until transmission completes,
/// so both remain valid and unmoved for the entire duration.
fn send_and_wait<E: RawEncoder>(
    channel: &mut TxChannelDriver,
    encoder: &mut E,
    data: &[E::Item],
    config: &TransmitConfig,
    timeout: Option<Duration>,
) -> Result<(), esp_idf_svc::sys::EspError> {
    unsafe {
        channel.start_send(encoder, data, config)?;
    }
    channel.wait_all_done(timeout)
}

impl<'d> Esp32LedDisplay<'d> {
    pub fn new(pin: impl OutputPin + 'd, palette: LedPalette) -> Result<Self, LedDisplayError> {
        let tx_config = TxChannelConfig {
            resolution: RMT_RESOLUTION_HZ.Hz(),
            ..Default::default()
        };
        let channel = TxChannelDriver::new(pin, &tx_config)
            .map_err(|e| LedDisplayError::DriverInit(format!("{e}")))?;

        let encoder_config = ws2812_bytes_encoder_config()?;
        let encoder = BytesEncoder::with_config(&encoder_config)
            .map_err(|e| LedDisplayError::DriverInit(format!("encoder: {e}")))?;

        Ok(Self {
            channel,
            encoder,
            buffer: [palette.off; NUM_LEDS],
            palette,
        })
    }

    fn color_for(&self, feedback: SquareFeedback) -> Rgb8 {
        match feedback {
            SquareFeedback::Destination => self.palette.destination,
            SquareFeedback::Capture => self.palette.capture,
            SquareFeedback::Origin => self.palette.origin,
            SquareFeedback::Check => self.palette.check,
            SquareFeedback::Checker => self.palette.checker,
            SquareFeedback::Victory => self.palette.victory,
            SquareFeedback::Stalemate => self.palette.stalemate,
        }
    }

    fn flush(&mut self) -> Result<(), LedDisplayError> {
        let grb_bytes: Vec<u8> = self.buffer.iter().flat_map(|c| [c.g, c.r, c.b]).collect();

        send_and_wait(
            &mut self.channel,
            &mut self.encoder,
            &grb_bytes,
            &TransmitConfig::default(),
            Some(Duration::from_millis(1000)),
        )
        .map_err(|e| LedDisplayError::WriteFailed(format!("{e}")))?;

        Ok(())
    }
}

/// Squares forming a hollow 4×4 ring in the board center (c3–f6, excluding d4–e5).
const STATUS_RING: [shakmaty::Square; 12] = {
    use shakmaty::Square::*;
    [C3, D3, E3, F3, C4, F4, C5, F5, C6, D6, E6, F6]
};

impl BoardDisplay for Esp32LedDisplay<'_> {
    type Error = LedDisplayError;

    fn show(&mut self, feedback: &BoardFeedback) -> Result<(), Self::Error> {
        self.buffer.fill(self.palette.off);

        if let Some(status) = feedback.status() {
            let color = match status {
                StatusKind::Pending => self.palette.status_pending,
                StatusKind::Success => self.palette.status_success,
                StatusKind::Failure => self.palette.status_failure,
            };
            for sq in STATUS_RING {
                let (led1, led2) = leds_for_square(sq);
                self.buffer[led1] = color;
                self.buffer[led2] = color;
            }
        } else {
            for (sq, fb) in feedback.squares() {
                let color = self.color_for(fb);
                let (led1, led2) = leds_for_square(sq);
                self.buffer[led1] = color;
                self.buffer[led2] = color;
            }
        }

        self.flush()
    }
}
