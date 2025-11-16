use super::{Rgb, traits::*};

pub struct MockLedController {
    pixels: [Rgb; NUM_LEDS],
}
    
impl MockLedController {
    pub fn new() -> Self {
        Self {
            pixels: [Rgb::new(0, 0, 0); NUM_LEDS],
        }
    }
    
    #[allow(dead_code)]
    pub fn get_pixel(&self, index: usize) -> Option<Rgb> {
        self.pixels.get(index).copied()
    }
}

impl LedController for MockLedController {
    fn set_all(&mut self, colors: [Rgb; NUM_LEDS]) -> Result<(), ()> {
        self.pixels = colors;
        Ok(())
    }
    
    fn set_led(&mut self, index: usize, color: Rgb) -> Result<(), ()> {
        if index >= NUM_LEDS {
            return Err(());
        }
        self.pixels[index] = color;
        Ok(())
    }
    
    fn show(&mut self) -> Result<(), ()> {
        Ok(())
    }
}

pub struct MockHallSensorArray {
    bitboard: u64,
}

impl MockHallSensorArray {
    pub fn new() -> Self {
        Self { bitboard: 0 }
    }
    
    pub fn set_bitboard(&mut self, bitboard: u64) {
        self.bitboard = bitboard;
    }
}

impl HallSensorArray for MockHallSensorArray {
    fn read_all(&mut self) -> Result<u64, ()> {
        Ok(self.bitboard)
    }
}

pub struct MockChessBoard {
    pub leds: MockLedController,
    pub sensors: MockHallSensorArray,
}

impl MockChessBoard {
    pub fn new() -> Self {
        Self {
            leds: MockLedController::new(),
            sensors: MockHallSensorArray::new(),
        }
    }
    
    pub fn setup_initial_position(&mut self) {
        let initial_position = 0xFFFF00000000FFFF;
        self.sensors.set_bitboard(initial_position);
    }
}

impl ChessBoardHardware for MockChessBoard {
    type Led = MockLedController;
    type Sensors = MockHallSensorArray;
    
    fn leds(&mut self) -> &mut Self::Led {
        &mut self.leds
    }
    
    fn sensors(&mut self) -> &mut Self::Sensors {
        &mut self.sensors
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_square_leds_mapping() {
        assert_eq!(square_leds(0), [0, 1, 2, 3]);
        assert_eq!(square_leds(1), [4, 5, 6, 7]);
        assert_eq!(square_leds(63), [252, 253, 254, 255]);
    }
    
    #[test]
    fn test_set_square() {
        let mut leds = MockLedController::new();
        let red = Rgb::new(255, 0, 0);
        
        leds.set_square(0, [red; 4]).unwrap();
        
        for i in 0..4 {
            assert_eq!(leds.get_pixel(i), Some(red));
        }
        assert_eq!(leds.get_pixel(4), Some(Rgb::new(0, 0, 0)));
    }
    
    #[test]
    fn test_read_square() {
        let mut sensors = MockHallSensorArray::new();
        sensors.set_bitboard(0b1010);
        
        assert_eq!(sensors.read_square(0).unwrap(), false);
        assert_eq!(sensors.read_square(1).unwrap(), true);
        assert_eq!(sensors.read_square(2).unwrap(), false);
        assert_eq!(sensors.read_square(3).unwrap(), true);
    }
}
