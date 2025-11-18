use crate::game_state::{Bitboard, PieceSensor, Square};

/// Mock sensor for testing and development on non-ESP32 targets.
///
/// Maintains an in-memory bitboard that can be toggled via the terminal interface.
#[derive(Debug, Clone, Default)]
pub struct MockPieceSensor {
    bitboard: Bitboard,
}

impl MockPieceSensor {
    /// Creates a new mock sensor with an empty board.
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggles the piece presence at the given square.
    pub fn toggle(&mut self, square: Square) {
        self.bitboard.toggle(square);
    }
}

impl PieceSensor for MockPieceSensor {
    fn read_positions(&mut self) -> Bitboard {
        self.bitboard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_sensor_empty_on_creation() {
        let mut sensor = MockPieceSensor::new();
        assert_eq!(sensor.read_positions().value(), 0);
    }

    #[test]
    fn test_mock_sensor_default() {
        let mut sensor = MockPieceSensor::default();
        assert_eq!(sensor.read_positions().value(), 0);
    }

    #[test]
    fn test_mock_sensor_toggle() {
        let mut sensor = MockPieceSensor::new();
        let square = Square::new(27).unwrap(); // d4

        sensor.toggle(square);
        assert_eq!(sensor.read_positions().value(), 1 << 27);

        sensor.toggle(square);
        assert_eq!(sensor.read_positions().value(), 0);
    }

    #[test]
    fn test_mock_sensor_multiple_toggles() {
        let mut sensor = MockPieceSensor::new();

        sensor.toggle(Square::new(0).unwrap()); // a1
        sensor.toggle(Square::new(7).unwrap()); // h1
        sensor.toggle(Square::new(63).unwrap()); // h8

        let bb = sensor.read_positions();
        assert_eq!(bb.value(), 0x8000000000000081);
        assert_eq!(bb.value().count_ones(), 3);
    }
}
