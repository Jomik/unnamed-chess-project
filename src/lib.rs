pub mod game_logic;

#[cfg(target_os = "espidf")]
pub mod esp32;

#[cfg(not(target_os = "espidf"))]
pub mod mock;
