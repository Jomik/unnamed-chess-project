#[cfg(target_os = "espidf")]
mod esp32;

#[cfg(target_os = "espidf")]
pub use esp32::Esp32PieceSensor;

#[cfg(not(target_os = "espidf"))]
mod mock;

#[cfg(not(target_os = "espidf"))]
pub use mock::MockPieceSensor;
