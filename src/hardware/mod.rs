#[cfg(target_arch = "xtensa")]
pub mod esp32;

#[cfg(target_arch = "xtensa")]
pub use esp32::Esp32ChessBoard;

#[cfg(not(target_arch = "xtensa"))]
pub mod mock;
#[cfg(not(target_arch = "xtensa"))]
pub use mock::MockChessBoard;

pub mod traits;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}
