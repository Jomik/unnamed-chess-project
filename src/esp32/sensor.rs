use crate::PieceSensor;
use crate::esp32::config::SensorConfig;
use esp_idf_svc::hal::adc::attenuation;
use esp_idf_svc::hal::adc::oneshot::config::AdcChannelConfig;
use esp_idf_svc::hal::adc::oneshot::{AdcChannelDriver, AdcDriver};
use esp_idf_svc::hal::adc::{AdcChannel, AdcUnit};
use esp_idf_svc::hal::gpio::{ADCPin, Output, OutputPin, PinDriver};
use shakmaty::{Bitboard, ByColor, File, Rank, Square};

const NUM_MUXES: usize = 4;
const CHANNELS_PER_MUX: usize = 16;
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SensorError {
    #[error("ADC read error on mux {mux_idx} channel {channel}")]
    AdcRead { mux_idx: usize, channel: u8 },
    #[error("GPIO error setting mux address")]
    GpioError,
}

/// Raw ADC millivolt readings for all 64 squares from a single scan.
///
/// Indexed by [`Square`] as `usize`. shakmaty's `Square` is `repr(u8)`
/// with A1=0, B1=1, ..., H8=63.
#[derive(Debug, Clone)]
pub struct RawScan {
    pub mv: [u16; 64],
}

impl RawScan {
    /// Signed deviation from baseline for a given square.
    /// Positive = above baseline (white piece direction),
    /// negative = below baseline (black piece direction).
    pub fn deviation(&self, sq: Square, baseline_mv: u16) -> i32 {
        self.mv[sq as usize] as i32 - baseline_mv as i32
    }
}

/// Drives the 4 shared mux address lines (S0-S3) that select which of the
/// 16 channels is active on all CD74HC4067 chips simultaneously.
///
/// `PinDriver` in esp-idf-hal 0.46 is no longer generic over pin type,
/// so this struct needs no generics.
struct MuxAddressLines {
    s0: PinDriver<'static, Output>,
    s1: PinDriver<'static, Output>,
    s2: PinDriver<'static, Output>,
    s3: PinDriver<'static, Output>,
}

impl MuxAddressLines {
    /// Set the 4-bit mux address (0..15). All 4 mux chips share these
    /// address lines, so setting the address selects the same channel
    /// index on every mux simultaneously.
    fn select(&mut self, channel: u8) -> Result<(), SensorError> {
        self.s0
            .set_level((channel & 1 != 0).into())
            .map_err(|_| SensorError::GpioError)?;
        self.s1
            .set_level((channel & 2 != 0).into())
            .map_err(|_| SensorError::GpioError)?;
        self.s2
            .set_level((channel & 4 != 0).into())
            .map_err(|_| SensorError::GpioError)?;
        self.s3
            .set_level((channel & 8 != 0).into())
            .map_err(|_| SensorError::GpioError)?;
        Ok(())
    }
}

/// DRV5055A3 analog Hall-effect sensor array, scanned via 4 CD74HC4067
/// analog multiplexers.
///
/// Each mux handles 2 rows of the board (16 sensors). The 4 mux output
/// lines connect to ADC1 channels. Per-square ADC readings distinguish
/// piece color by comparing against VCC/2: output > baseline = white
/// piece (south pole), output < baseline = black piece (north pole).
///
/// Lifetime `'a` ties this to the `AdcDriver` created in `main()`.
pub struct Esp32PieceSensor<'a> {
    mux_channels: [MuxChannel<'a>; NUM_MUXES],
    address_lines: MuxAddressLines,
    config: SensorConfig,
}

/// Type-erased ADC channel that can read millivolt values.
///
/// We need this because each `AdcChannelDriver` has a different pin type
/// parameter (Gpio4, Gpio5, etc.), so we can't store them in an array
/// directly. The closure captures the concrete type.
struct MuxChannel<'a> {
    read_fn: Box<dyn FnMut() -> Result<u16, SensorError> + 'a>,
}

impl<'a> MuxChannel<'a> {
    fn new<C, M>(mut driver: AdcChannelDriver<'a, C, M>, mux_idx: usize) -> Self
    where
        C: AdcChannel + 'a,
        M: std::borrow::Borrow<AdcDriver<'a, C::AdcUnit>> + 'a,
    {
        Self {
            read_fn: Box::new(move || {
                driver.read().map_err(|_| SensorError::AdcRead {
                    mux_idx,
                    channel: 0,
                })
            }),
        }
    }

    fn read(&mut self, channel: u8) -> Result<u16, SensorError> {
        (self.read_fn)().map_err(|_| SensorError::AdcRead {
            mux_idx: 0,
            channel,
        })
    }
}

/// Map a mux index (0..3) and mux channel (0..15) to a board square.
///
/// Each mux covers 2 consecutive rows. Channels 0-7 map to the first row,
/// channels 8-15 to the second row of that mux's pair.
///
/// Odd rows have mirrored mux channel wiring on the PCB: the physical
/// trace routing reverses the column order. This is a fixed property of
/// the board layout and will not change.
fn square_for(mux_idx: usize, channel: u8) -> Square {
    let row = mux_idx * 2 + (channel as usize / 8);
    let raw_col = (channel as usize) % 8;
    let col = if row % 2 == 1 { 7 - raw_col } else { raw_col };
    Square::from_coords(File::new(col as u32), Rank::new(row as u32))
}

impl<'a> Esp32PieceSensor<'a> {
    /// Create a new sensor driver from individual peripherals.
    ///
    /// The `AdcDriver` must be created by the caller (typically `main()`)
    /// and outlive this struct. This avoids `Box::leak` by making the
    /// borrow relationship explicit through the `'a` lifetime.
    ///
    /// Takes individual pin peripherals because ESP32 variants differ in
    /// available pins — a future board revision just changes the call site.
    #[allow(clippy::too_many_arguments)]
    pub fn new<U, P0, P1, P2, P3>(
        adc: &'a AdcDriver<'a, U>,
        mux0_pin: P0,
        mux1_pin: P1,
        mux2_pin: P2,
        mux3_pin: P3,
        s0_pin: impl OutputPin + 'static,
        s1_pin: impl OutputPin + 'static,
        s2_pin: impl OutputPin + 'static,
        s3_pin: impl OutputPin + 'static,
        config: SensorConfig,
    ) -> Result<Self, SensorError>
    where
        U: AdcUnit + 'a,
        P0: ADCPin + 'a,
        P0::AdcChannel: AdcChannel<AdcUnit = U>,
        P1: ADCPin + 'a,
        P1::AdcChannel: AdcChannel<AdcUnit = U>,
        P2: ADCPin + 'a,
        P2::AdcChannel: AdcChannel<AdcUnit = U>,
        P3: ADCPin + 'a,
        P3::AdcChannel: AdcChannel<AdcUnit = U>,
    {
        let adc_config = AdcChannelConfig {
            attenuation: attenuation::DB_12,
            ..Default::default()
        };

        let ch0 = AdcChannelDriver::new(adc, mux0_pin, &adc_config)
            .map_err(|_| SensorError::GpioError)?;
        let ch1 = AdcChannelDriver::new(adc, mux1_pin, &adc_config)
            .map_err(|_| SensorError::GpioError)?;
        let ch2 = AdcChannelDriver::new(adc, mux2_pin, &adc_config)
            .map_err(|_| SensorError::GpioError)?;
        let ch3 = AdcChannelDriver::new(adc, mux3_pin, &adc_config)
            .map_err(|_| SensorError::GpioError)?;

        let mux_channels = [
            MuxChannel::new(ch0, 0),
            MuxChannel::new(ch1, 1),
            MuxChannel::new(ch2, 2),
            MuxChannel::new(ch3, 3),
        ];

        let address_lines = MuxAddressLines {
            s0: PinDriver::output(s0_pin).map_err(|_| SensorError::GpioError)?,
            s1: PinDriver::output(s1_pin).map_err(|_| SensorError::GpioError)?,
            s2: PinDriver::output(s2_pin).map_err(|_| SensorError::GpioError)?,
            s3: PinDriver::output(s3_pin).map_err(|_| SensorError::GpioError)?,
        };

        Ok(Self {
            mux_channels,
            address_lines,
            config,
        })
    }

    /// Perform a full 64-square scan and return raw millivolt readings.
    ///
    /// This is the low-level scan primitive. [`PieceSensor::read_positions`]
    /// delegates to this method and applies thresholding.
    pub fn read_raw(&mut self) -> Result<RawScan, SensorError> {
        let mut scan = RawScan { mv: [0; 64] };

        for channel in 0..CHANNELS_PER_MUX as u8 {
            self.address_lines.select(channel)?;
            esp_idf_svc::hal::delay::FreeRtos::delay_ms(self.config.settle_delay_ms);

            for (mux_idx, mux) in self.mux_channels.iter_mut().enumerate() {
                let mv = mux.read(channel)?;
                let sq = square_for(mux_idx, channel);
                scan.mv[sq as usize] = mv;
            }
        }

        Ok(scan)
    }
}

impl PieceSensor for Esp32PieceSensor<'_> {
    type Error = SensorError;

    fn read_positions(&mut self) -> Result<ByColor<Bitboard>, SensorError> {
        let scan = self.read_raw()?;
        let mut white = Bitboard::EMPTY;
        let mut black = Bitboard::EMPTY;

        for sq_idx in 0..64u32 {
            let mv = scan.mv[sq_idx as usize];
            let deviation = mv.abs_diff(self.config.baseline_mv);
            if deviation > self.config.threshold_mv {
                let sq = Square::new(sq_idx);
                if mv > self.config.baseline_mv {
                    white.toggle(sq);
                } else {
                    black.toggle(sq);
                }
            }
        }

        Ok(ByColor { white, black })
    }
}
