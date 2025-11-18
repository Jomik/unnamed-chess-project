#[cfg(target_arch = "xtensa")]
pub mod esp32;

#[cfg(target_arch = "xtensa")]
pub use esp32::Esp32PieceSensor;

#[cfg(not(target_arch = "xtensa"))]
pub mod mock;

#[cfg(not(target_arch = "xtensa"))]
pub use mock::MockPieceSensor;
