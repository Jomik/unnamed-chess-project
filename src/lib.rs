pub mod game_logic;
pub mod hardware;

#[cfg(not(target_os = "espidf"))]
pub mod visualization;
