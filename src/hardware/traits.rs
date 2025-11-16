use super::Rgb;

/// Number of squares on the board
pub const NUM_SQUARES: usize = 64;

/// Number of LEDs per square
pub const LEDS_PER_SQUARE: usize = 4;

/// Total number of LEDs
pub const NUM_LEDS: usize = NUM_SQUARES * LEDS_PER_SQUARE;

/// Get the LED indices for a given square
/// Returns the 4 LED indices in order: [0, 1, 2, 3] relative to square base
pub fn square_leds(square: u8) -> [usize; LEDS_PER_SQUARE] {
    debug_assert!(square < NUM_SQUARES as u8);
    let base = (square as usize) * LEDS_PER_SQUARE;
    [base, base + 1, base + 2, base + 3]
}

pub trait LedController {
    /// Set all LEDs at once
    fn set_all(&mut self, colors: [Rgb; NUM_LEDS]) -> Result<(), ()>;

    /// Set LEDs for a specific square
    fn set_square(&mut self, square: u8, colors: [Rgb; LEDS_PER_SQUARE]) -> Result<(), ()> {
        if square >= NUM_SQUARES as u8 {
            return Err(());
        }
        let indices = square_leds(square);
        for (i, &led_idx) in indices.iter().enumerate() {
            self.set_led(led_idx, colors[i])?;
        }
        Ok(())
    }

    /// Set a single LED
    fn set_led(&mut self, index: usize, color: Rgb) -> Result<(), ()>;

    /// Commit changes to hardware
    fn show(&mut self) -> Result<(), ()>;
}

pub trait HallSensorArray {
    /// Read all sensors as a bitboard
    fn read_all(&mut self) -> Result<u64, ()>;
}

pub trait ChessBoardHardware {
    type Led: LedController;
    type Sensors: HallSensorArray;

    fn leds(&mut self) -> &mut Self::Led;
    fn sensors(&mut self) -> &mut Self::Sensors;
}
