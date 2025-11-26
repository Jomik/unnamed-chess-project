pub mod game_logic;

#[cfg(target_os = "espidf")]
pub mod hardware;

#[cfg(not(target_os = "espidf"))]
pub mod visualization;
