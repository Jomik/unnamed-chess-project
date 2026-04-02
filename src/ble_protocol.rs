use shakmaty::Color;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("unknown player type byte: 0x{0:02x}")]
    UnknownPlayerType(u8),
    #[error("insufficient data: need {needed} bytes, got {got}")]
    InsufficientData { needed: usize, got: usize },
    #[error("Lichess AI level {0} is out of range (must be 1–8)")]
    LevelOutOfRange(u8),
    #[error("unknown match control action byte: 0x{0:02x}")]
    UnknownAction(u8),
    #[error("unknown color byte: 0x{0:02x}")]
    UnknownColor(u8),
}

/// Sentinel byte indicating a player slot has not yet been configured.
///
/// White Player and Black Player characteristics read as this value on boot
/// and after each game ends. `StartGame` is rejected while either slot
/// holds this value.
pub const UNSET_BYTE: u8 = 0xFF;

/// The type of player assigned to a side.
///
/// Wire encoding (tagged binary):
/// - Human:           `[0x00]`
/// - Embedded Engine: `[0x01]`
/// - Lichess AI:      `[0x02] [level: u8 (1–8)]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerConfig {
    Human,
    Embedded,
    LichessAi { level: u8 },
}

impl PlayerConfig {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            PlayerConfig::Human => vec![0x00],
            PlayerConfig::Embedded => vec![0x01],
            PlayerConfig::LichessAi { level } => vec![0x02, *level],
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.is_empty() {
            return Err(ProtocolError::InsufficientData { needed: 1, got: 0 });
        }
        match bytes[0] {
            0x00 => Ok(PlayerConfig::Human),
            0x01 => Ok(PlayerConfig::Embedded),
            0x02 => {
                if bytes.len() < 2 {
                    return Err(ProtocolError::InsufficientData {
                        needed: 2,
                        got: bytes.len(),
                    });
                }
                let level = bytes[1];
                if !(1..=8).contains(&level) {
                    return Err(ProtocolError::LevelOutOfRange(level));
                }
                Ok(PlayerConfig::LichessAi { level })
            }
            other => Err(ProtocolError::UnknownPlayerType(other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GameStatus {
    NotStarted = 0x00,
    InProgress = 0x01,
    Checkmate = 0x02,
    Stalemate = 0x03,
    Resignation = 0x04,
    Draw = 0x05,
}

impl From<GameStatus> for u8 {
    fn from(status: GameStatus) -> u8 {
        status as u8
    }
}

/// The current game state, sent to the app as a BLE notification.
///
/// Wire encoding: `[status: u8, turn: u8]`
///
/// `turn` is `0x00` for white, `0x01` for black.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameState {
    pub status: GameStatus,
    pub turn: Color,
}

impl GameState {
    pub fn not_started() -> Self {
        Self {
            status: GameStatus::NotStarted,
            turn: Color::White,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let turn_byte: u8 = match self.turn {
            Color::White => 0x00,
            Color::Black => 0x01,
        };
        vec![u8::from(self.status), turn_byte]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BleCommand {
    SetWhitePlayer(PlayerConfig),
    SetBlackPlayer(PlayerConfig),
    StartGame,
    Resign { color: Color },
}

impl BleCommand {
    /// Parse a Match Control characteristic write.
    ///
    /// Format: `[action: u8, player: u8]`
    /// - action `0x00` = resign
    /// - player `0x00` = white, `0x01` = black
    pub fn parse_match_control(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < 2 {
            return Err(ProtocolError::InsufficientData {
                needed: 2,
                got: bytes.len(),
            });
        }
        let action = bytes[0];
        let color = parse_color(bytes[1])?;
        match action {
            0x00 => Ok(BleCommand::Resign { color }),
            other => Err(ProtocolError::UnknownAction(other)),
        }
    }
}

/// Parse a color byte: `0x00` = white, `0x01` = black.
fn parse_color(byte: u8) -> Result<Color, ProtocolError> {
    match byte {
        0x00 => Ok(Color::White),
        0x01 => Ok(Color::Black),
        other => Err(ProtocolError::UnknownColor(other)),
    }
}

/// The result of processing a BLE command (Start Game or Match Control).
///
/// Wire encoding: `[ok: u8, msg_len: u8, msg_bytes...]`
/// - `ok` is `0x00` for success, `0x01` for error.
/// - On success, `msg_len` is `0x00` and no message bytes follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub ok: bool,
    pub message: String,
}

impl CommandResult {
    pub fn success() -> Self {
        Self {
            ok: true,
            message: String::new(),
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: msg.into(),
        }
    }

    /// Panics if message exceeds 255 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let msg_bytes = self.message.as_bytes();
        let msg_len =
            u8::try_from(msg_bytes.len()).expect("CommandResult message exceeds 255 bytes");
        let ok_byte: u8 = if self.ok { 0x00 } else { 0x01 };

        let mut out = Vec::with_capacity(2 + msg_bytes.len());
        out.push(ok_byte);
        out.push(msg_len);
        out.extend_from_slice(msg_bytes);
        out
    }
}

/// BLE GATT UUID constants.
///
/// All UUIDs share the base `3d6343a2-xxxx-44ea-8fc2-3568d7216866`.
/// They are assigned once and must not change — bonded iOS devices
/// cache discovered services.
pub mod uuids {
    pub const GAME_SERVICE: &str = "3d6343a2-1001-44ea-8fc2-3568d7216866";
    pub const WHITE_PLAYER: &str = "3d6343a2-1002-44ea-8fc2-3568d7216866";
    pub const BLACK_PLAYER: &str = "3d6343a2-1003-44ea-8fc2-3568d7216866";
    pub const START_GAME: &str = "3d6343a2-1004-44ea-8fc2-3568d7216866";
    pub const MATCH_CONTROL: &str = "3d6343a2-1005-44ea-8fc2-3568d7216866";
    pub const GAME_STATE: &str = "3d6343a2-1006-44ea-8fc2-3568d7216866";
    pub const COMMAND_RESULT: &str = "3d6343a2-1007-44ea-8fc2-3568d7216866";

    pub const WIFI_SERVICE: &str = "3d6343a2-2001-44ea-8fc2-3568d7216866";
    pub const WIFI_CONFIG: &str = "3d6343a2-2002-44ea-8fc2-3568d7216866";
    pub const WIFI_STATUS: &str = "3d6343a2-2003-44ea-8fc2-3568d7216866";

    pub const LICHESS_SERVICE: &str = "3d6343a2-3001-44ea-8fc2-3568d7216866";
    pub const LICHESS_TOKEN: &str = "3d6343a2-3002-44ea-8fc2-3568d7216866";
    pub const LICHESS_STATUS: &str = "3d6343a2-3003-44ea-8fc2-3568d7216866";
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── PlayerConfig::encode ──────────────────────────────────────────────────

    #[test]
    fn encode_human_player() {
        assert_eq!(PlayerConfig::Human.encode(), vec![0x00]);
    }

    #[test]
    fn encode_embedded_player() {
        assert_eq!(PlayerConfig::Embedded.encode(), vec![0x01]);
    }

    #[test]
    fn encode_lichess_ai_player() {
        assert_eq!(
            PlayerConfig::LichessAi { level: 5 }.encode(),
            vec![0x02, 0x05]
        );
    }

    #[test]
    fn encode_decode_roundtrip() {
        let configs = [
            PlayerConfig::Human,
            PlayerConfig::Embedded,
            PlayerConfig::LichessAi { level: 1 },
            PlayerConfig::LichessAi { level: 8 },
        ];
        for config in &configs {
            let encoded = config.encode();
            let decoded = PlayerConfig::from_bytes(&encoded).expect("roundtrip should succeed");
            assert_eq!(&decoded, config);
        }
    }

    // ── PlayerConfig::from_bytes ──────────────────────────────────────────────

    #[test]
    fn parse_human_player() {
        let result = PlayerConfig::from_bytes(&[0x00]);
        assert_eq!(result, Ok(PlayerConfig::Human));
    }

    #[test]
    fn parse_embedded_player() {
        let result = PlayerConfig::from_bytes(&[0x01]);
        assert_eq!(result, Ok(PlayerConfig::Embedded));
    }

    #[test]
    fn parse_lichess_ai_level_4() {
        let result = PlayerConfig::from_bytes(&[0x02, 0x04]);
        assert_eq!(result, Ok(PlayerConfig::LichessAi { level: 4 }));
    }

    #[test]
    fn parse_lichess_ai_level_1_boundary() {
        let result = PlayerConfig::from_bytes(&[0x02, 0x01]);
        assert_eq!(result, Ok(PlayerConfig::LichessAi { level: 1 }));
    }

    #[test]
    fn parse_lichess_ai_level_8_boundary() {
        let result = PlayerConfig::from_bytes(&[0x02, 0x08]);
        assert_eq!(result, Ok(PlayerConfig::LichessAi { level: 8 }));
    }

    #[test]
    fn reject_unset_byte() {
        let result = PlayerConfig::from_bytes(&[UNSET_BYTE]);
        assert!(matches!(
            result,
            Err(ProtocolError::UnknownPlayerType(0xFF))
        ));
    }

    #[test]
    fn reject_unknown_player_type() {
        let result = PlayerConfig::from_bytes(&[0x42]);
        assert!(matches!(
            result,
            Err(ProtocolError::UnknownPlayerType(0x42))
        ));
    }

    #[test]
    fn reject_empty_bytes() {
        let result = PlayerConfig::from_bytes(&[]);
        assert!(matches!(
            result,
            Err(ProtocolError::InsufficientData { needed: 1, got: 0 })
        ));
    }

    #[test]
    fn reject_lichess_missing_level_byte() {
        let result = PlayerConfig::from_bytes(&[0x02]);
        assert!(matches!(
            result,
            Err(ProtocolError::InsufficientData { needed: 2, got: 1 })
        ));
    }

    #[test]
    fn reject_lichess_level_zero() {
        let result = PlayerConfig::from_bytes(&[0x02, 0x00]);
        assert!(matches!(result, Err(ProtocolError::LevelOutOfRange(0))));
    }

    #[test]
    fn reject_lichess_level_nine() {
        let result = PlayerConfig::from_bytes(&[0x02, 0x09]);
        assert!(matches!(result, Err(ProtocolError::LevelOutOfRange(9))));
    }

    // ── GameState::encode ─────────────────────────────────────────────────────

    #[test]
    fn encode_not_started_state() {
        let state = GameState::not_started();
        let encoded = state.encode();

        assert_eq!(encoded, vec![0x00, 0x00]); // NotStarted, White
        assert_eq!(encoded.len(), 2);
    }

    #[test]
    fn encode_in_progress_state_black_turn() {
        let state = GameState {
            status: GameStatus::InProgress,
            turn: Color::Black,
        };
        let encoded = state.encode();

        assert_eq!(encoded[0], 0x01); // InProgress
        assert_eq!(encoded[1], 0x01); // Black
        assert_eq!(encoded.len(), 2);
    }

    #[test]
    fn encode_game_state_byte_layout() {
        // Verify the exact byte structure: [status, turn]
        let state = GameState {
            status: GameStatus::Checkmate,
            turn: Color::White,
        };
        let encoded = state.encode();
        assert_eq!(encoded, vec![0x02, 0x00]);
    }

    // ── BleCommand::parse_match_control ──────────────────────────────────────

    #[test]
    fn parse_resign_white() {
        let result = BleCommand::parse_match_control(&[0x00, 0x00]);
        assert_eq!(
            result,
            Ok(BleCommand::Resign {
                color: Color::White
            })
        );
    }

    #[test]
    fn parse_resign_black() {
        let result = BleCommand::parse_match_control(&[0x00, 0x01]);
        assert_eq!(
            result,
            Ok(BleCommand::Resign {
                color: Color::Black
            })
        );
    }

    #[test]
    fn reject_unknown_action() {
        let result = BleCommand::parse_match_control(&[0x01, 0x00]);
        assert!(matches!(result, Err(ProtocolError::UnknownAction(0x01))));
    }

    #[test]
    fn reject_unknown_color() {
        let result = BleCommand::parse_match_control(&[0x00, 0x02]);
        assert!(matches!(result, Err(ProtocolError::UnknownColor(0x02))));
    }

    #[test]
    fn reject_match_control_insufficient_data() {
        let result = BleCommand::parse_match_control(&[0x00]);
        assert!(matches!(
            result,
            Err(ProtocolError::InsufficientData { needed: 2, got: 1 })
        ));
    }

    #[test]
    fn reject_match_control_empty() {
        let result = BleCommand::parse_match_control(&[]);
        assert!(matches!(
            result,
            Err(ProtocolError::InsufficientData { needed: 2, got: 0 })
        ));
    }

    // ── CommandResult::encode ─────────────────────────────────────────────────

    #[test]
    fn encode_success_result() {
        let result = CommandResult::success();
        assert_eq!(result.encode(), vec![0x00, 0x00]);
    }

    #[test]
    fn encode_error_result() {
        let result = CommandResult::error("oops");
        let encoded = result.encode();
        assert_eq!(encoded[0], 0x01); // error
        assert_eq!(encoded[1], 4); // "oops" is 4 bytes
        assert_eq!(&encoded[2..], b"oops");
    }

    #[test]
    fn encode_error_result_byte_layout() {
        let result = CommandResult::error("hi");
        assert_eq!(result.encode(), vec![0x01, 0x02, b'h', b'i']);
    }

    // ── GameStatus u8 conversions ─────────────────────────────────────────────

    #[test]
    fn game_status_values() {
        assert_eq!(u8::from(GameStatus::NotStarted), 0x00);
        assert_eq!(u8::from(GameStatus::InProgress), 0x01);
        assert_eq!(u8::from(GameStatus::Checkmate), 0x02);
        assert_eq!(u8::from(GameStatus::Stalemate), 0x03);
        assert_eq!(u8::from(GameStatus::Resignation), 0x04);
        assert_eq!(u8::from(GameStatus::Draw), 0x05);
    }

    // ── UUID constants ────────────────────────────────────────────────────────

    #[test]
    fn uuid_registry_has_correct_values() {
        assert_eq!(uuids::GAME_SERVICE, "3d6343a2-1001-44ea-8fc2-3568d7216866");
        assert_eq!(uuids::WHITE_PLAYER, "3d6343a2-1002-44ea-8fc2-3568d7216866");
        assert_eq!(uuids::BLACK_PLAYER, "3d6343a2-1003-44ea-8fc2-3568d7216866");
        assert_eq!(uuids::START_GAME, "3d6343a2-1004-44ea-8fc2-3568d7216866");
        assert_eq!(uuids::MATCH_CONTROL, "3d6343a2-1005-44ea-8fc2-3568d7216866");
        assert_eq!(uuids::GAME_STATE, "3d6343a2-1006-44ea-8fc2-3568d7216866");
        assert_eq!(
            uuids::COMMAND_RESULT,
            "3d6343a2-1007-44ea-8fc2-3568d7216866"
        );
        assert_eq!(uuids::WIFI_SERVICE, "3d6343a2-2001-44ea-8fc2-3568d7216866");
        assert_eq!(uuids::WIFI_CONFIG, "3d6343a2-2002-44ea-8fc2-3568d7216866");
        assert_eq!(uuids::WIFI_STATUS, "3d6343a2-2003-44ea-8fc2-3568d7216866");
        assert_eq!(
            uuids::LICHESS_SERVICE,
            "3d6343a2-3001-44ea-8fc2-3568d7216866"
        );
        assert_eq!(uuids::LICHESS_TOKEN, "3d6343a2-3002-44ea-8fc2-3568d7216866");
        assert_eq!(
            uuids::LICHESS_STATUS,
            "3d6343a2-3003-44ea-8fc2-3568d7216866"
        );
    }
}
