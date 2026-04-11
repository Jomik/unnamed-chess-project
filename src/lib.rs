use shakmaty::{Bitboard, ByColor};

pub mod ble_protocol;
pub mod board_api;
pub mod feedback;
pub mod lichess;
pub mod player;
pub mod session;
pub mod setup;

/// Trait for reading piece positions from the board.
///
/// Abstracts over hardware sensors (ESP32) and scripted test inputs,
/// providing a uniform interface for the game session.
pub trait PieceSensor {
    /// Error type for sensor read failures.
    type Error: std::fmt::Debug + std::fmt::Display;

    /// Read current per-color piece positions from the board.
    fn read_positions(&mut self) -> Result<ByColor<Bitboard>, Self::Error>;
}

/// Trait for displaying board feedback to the player.
///
/// Abstracts over LED hardware (ESP32) for displaying board feedback,
/// providing a uniform interface for the output side of the
/// game loop. Mirrors [`PieceSensor`] on the input side.
pub trait BoardDisplay {
    /// Error type for display update failures.
    type Error: std::fmt::Debug + std::fmt::Display;

    /// Show the given feedback state on the display.
    ///
    /// Implementations map [`feedback::SquareFeedback`] variants
    /// to hardware-specific output (LED colors, etc.).
    fn show(&mut self, feedback: &feedback::BoardFeedback) -> Result<(), Self::Error>;
}

#[cfg(target_os = "espidf")]
pub mod esp32;

#[cfg(test)]
pub mod testutil;
