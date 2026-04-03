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
    #[error("unknown auth mode byte: 0x{0:02x}")]
    UnknownAuthMode(u8),
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
    Idle = 0x00,
    AwaitingPieces = 0x01,
    InProgress = 0x02,
    Checkmate = 0x03,
    Stalemate = 0x04,
    Resignation = 0x05,
    Draw = 0x06,
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
    pub fn idle() -> Self {
        Self {
            status: GameStatus::Idle,
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
    ConfigureWifi(WifiConfig),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandSource {
    StartGame = 0x00,
    MatchControl = 0x01,
}

/// The result of processing a BLE command (Start Game or Match Control).
///
/// Wire encoding: `[ok: u8, command: u8, msg_len: u8, msg_bytes...]`
/// - `ok` is `0x00` for success, `0x01` for error.
/// - `command` identifies which command produced this result.
/// - On success, `msg_len` is `0x00` and no message bytes follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub ok: bool,
    pub source: CommandSource,
    pub message: String,
}

impl CommandResult {
    pub fn success(source: CommandSource) -> Self {
        Self {
            ok: true,
            source,
            message: String::new(),
        }
    }

    pub fn error(source: CommandSource, msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            source,
            message: msg.into(),
        }
    }

    /// Panics if message exceeds 255 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let msg_bytes = self.message.as_bytes();
        let msg_len =
            u8::try_from(msg_bytes.len()).expect("CommandResult message exceeds 255 bytes");
        let ok_byte: u8 = if self.ok { 0x00 } else { 0x01 };

        let mut out = Vec::with_capacity(3 + msg_bytes.len());
        out.push(ok_byte);
        out.push(self.source as u8);
        out.push(msg_len);
        out.extend_from_slice(msg_bytes);
        out
    }
}

/// WiFi authentication mode.
///
/// Wire encoding:
/// - Open:  0x00
/// - WPA2:  0x01
/// - WPA3:  0x02
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WifiAuthMode {
    Open = 0x00,
    Wpa2 = 0x01,
    Wpa3 = 0x02,
}

impl WifiAuthMode {
    pub fn from_byte(byte: u8) -> Result<Self, ProtocolError> {
        match byte {
            0x00 => Ok(WifiAuthMode::Open),
            0x01 => Ok(WifiAuthMode::Wpa2),
            0x02 => Ok(WifiAuthMode::Wpa3),
            other => Err(ProtocolError::UnknownAuthMode(other)),
        }
    }
}

impl From<WifiAuthMode> for u8 {
    fn from(mode: WifiAuthMode) -> u8 {
        mode as u8
    }
}

/// WiFi configuration sent via BLE.
///
/// Wire encoding: `[ssid_len: u8, ssid_bytes..., pass_len: u8, pass_bytes..., auth_mode: u8]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiConfig {
    pub ssid: String,
    pub password: String,
    pub auth_mode: WifiAuthMode,
}

impl WifiConfig {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.is_empty() {
            return Err(ProtocolError::InsufficientData { needed: 1, got: 0 });
        }

        let ssid_len = bytes[0] as usize;
        let ssid_start = 1;
        let ssid_end = ssid_start + ssid_len;

        if bytes.len() < ssid_end {
            return Err(ProtocolError::InsufficientData {
                needed: ssid_end,
                got: bytes.len(),
            });
        }

        let ssid = String::from_utf8_lossy(&bytes[ssid_start..ssid_end]).into_owned();

        let pass_len_pos = ssid_end;
        if bytes.len() < pass_len_pos + 1 {
            return Err(ProtocolError::InsufficientData {
                needed: pass_len_pos + 1,
                got: bytes.len(),
            });
        }

        let pass_len = bytes[pass_len_pos] as usize;
        let pass_start = pass_len_pos + 1;
        let pass_end = pass_start + pass_len;

        if bytes.len() < pass_end {
            return Err(ProtocolError::InsufficientData {
                needed: pass_end,
                got: bytes.len(),
            });
        }

        let password = String::from_utf8_lossy(&bytes[pass_start..pass_end]).into_owned();

        let auth_pos = pass_end;
        if bytes.len() < auth_pos + 1 {
            return Err(ProtocolError::InsufficientData {
                needed: auth_pos + 1,
                got: bytes.len(),
            });
        }

        let auth_mode = WifiAuthMode::from_byte(bytes[auth_pos])?;

        Ok(WifiConfig {
            ssid,
            password,
            auth_mode,
        })
    }
}

/// WiFi connection state.
///
/// Wire encoding:
/// - Disconnected: 0x00
/// - Connecting:   0x01
/// - Connected:    0x02
/// - Failed:       0x03
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WifiState {
    Disconnected = 0x00,
    Connecting = 0x01,
    Connected = 0x02,
    Failed = 0x03,
}

impl From<WifiState> for u8 {
    fn from(state: WifiState) -> u8 {
        state as u8
    }
}

/// WiFi status sent to the app via BLE notification.
///
/// Wire encoding: `[state: u8, msg_len: u8, msg_bytes...]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiStatus {
    pub state: WifiState,
    pub message: String,
}

impl WifiStatus {
    pub fn disconnected() -> Self {
        Self {
            state: WifiState::Disconnected,
            message: String::new(),
        }
    }

    pub fn connecting() -> Self {
        Self {
            state: WifiState::Connecting,
            message: String::new(),
        }
    }

    pub fn connected() -> Self {
        Self {
            state: WifiState::Connected,
            message: String::new(),
        }
    }

    pub fn failed(msg: impl Into<String>) -> Self {
        Self {
            state: WifiState::Failed,
            message: msg.into(),
        }
    }

    /// Panics if message exceeds 255 bytes.
    pub fn encode(&self) -> Vec<u8> {
        let msg_bytes = self.message.as_bytes();
        let msg_len = u8::try_from(msg_bytes.len()).expect("WifiStatus message exceeds 255 bytes");

        let mut out = Vec::with_capacity(2 + msg_bytes.len());
        out.push(u8::from(self.state));
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

    #[test]
    fn encode_idle_state() {
        let state = GameState::idle();
        let encoded = state.encode();

        assert_eq!(encoded, vec![0x00, 0x00]);
        assert_eq!(encoded.len(), 2);
    }

    #[test]
    fn encode_in_progress_state_black_turn() {
        let state = GameState {
            status: GameStatus::InProgress,
            turn: Color::Black,
        };
        let encoded = state.encode();

        assert_eq!(encoded[0], 0x02); // InProgress
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
        assert_eq!(encoded, vec![0x03, 0x00]);
    }

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

    #[test]
    fn encode_success_start_game() {
        let result = CommandResult::success(CommandSource::StartGame);
        assert_eq!(result.encode(), vec![0x00, 0x00, 0x00]);
    }

    #[test]
    fn encode_success_match_control() {
        let result = CommandResult::success(CommandSource::MatchControl);
        assert_eq!(result.encode(), vec![0x00, 0x01, 0x00]);
    }

    #[test]
    fn encode_error_start_game() {
        let result = CommandResult::error(CommandSource::StartGame, "oops");
        let encoded = result.encode();
        assert_eq!(encoded[0], 0x01); // error
        assert_eq!(encoded[1], 0x00); // StartGame
        assert_eq!(encoded[2], 4); // "oops" is 4 bytes
        assert_eq!(&encoded[3..], b"oops");
    }

    #[test]
    fn encode_error_match_control_byte_layout() {
        let result = CommandResult::error(CommandSource::MatchControl, "hi");
        assert_eq!(result.encode(), vec![0x01, 0x01, 0x02, b'h', b'i']);
    }

    #[test]
    fn encode_error_start_game_byte_layout() {
        let result = CommandResult::error(CommandSource::StartGame, "hi");
        assert_eq!(result.encode(), vec![0x01, 0x00, 0x02, b'h', b'i']);
    }

    #[test]
    fn game_status_values() {
        assert_eq!(u8::from(GameStatus::Idle), 0x00);
        assert_eq!(u8::from(GameStatus::AwaitingPieces), 0x01);
        assert_eq!(u8::from(GameStatus::InProgress), 0x02);
        assert_eq!(u8::from(GameStatus::Checkmate), 0x03);
        assert_eq!(u8::from(GameStatus::Stalemate), 0x04);
        assert_eq!(u8::from(GameStatus::Resignation), 0x05);
        assert_eq!(u8::from(GameStatus::Draw), 0x06);
    }

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

    // WiFi config parsing tests
    #[test]
    fn parse_wifi_config_wpa2() {
        // [ssid_len=5][ssid="MyNet"][pass_len=7][pass="pass123"][auth=0x01]
        let bytes = [
            5u8, b'M', b'y', b'N', b'e', b't', 7, b'p', b'a', b's', b's', b'1', b'2', b'3', 0x01,
        ];
        let config = WifiConfig::from_bytes(&bytes).expect("should parse WPA2 config");
        assert_eq!(config.ssid, "MyNet");
        assert_eq!(config.password, "pass123");
        assert_eq!(config.auth_mode, WifiAuthMode::Wpa2);
    }

    #[test]
    fn parse_wifi_config_open() {
        // [ssid_len=4][ssid="Open"][pass_len=0][auth=0x00]
        let bytes = [4u8, b'O', b'p', b'e', b'n', 0, 0x00];
        let config = WifiConfig::from_bytes(&bytes).expect("should parse Open config");
        assert_eq!(config.ssid, "Open");
        assert_eq!(config.password, "");
        assert_eq!(config.auth_mode, WifiAuthMode::Open);
    }

    #[test]
    fn parse_wifi_config_wpa3() {
        // [ssid_len=7][ssid="Network"][pass_len=8][pass="password"][auth=0x02]
        let bytes = [
            7u8, b'N', b'e', b't', b'w', b'o', b'r', b'k', 8, b'p', b'a', b's', b's', b'w', b'o',
            b'r', b'd', 0x02,
        ];
        let config = WifiConfig::from_bytes(&bytes).expect("should parse WPA3 config");
        assert_eq!(config.ssid, "Network");
        assert_eq!(config.password, "password");
        assert_eq!(config.auth_mode, WifiAuthMode::Wpa3);
    }

    #[test]
    fn parse_wifi_config_empty_bytes() {
        let bytes = b"";
        let result = WifiConfig::from_bytes(bytes);
        assert!(matches!(
            result,
            Err(ProtocolError::InsufficientData { needed: 1, got: 0 })
        ));
    }

    #[test]
    fn parse_wifi_config_truncated_ssid() {
        // [ssid_len=5][ssid="abc"] - claims 5 bytes but only has 3
        let bytes = [5u8, b'a', b'b', b'c'];
        let result = WifiConfig::from_bytes(&bytes);
        assert!(matches!(
            result,
            Err(ProtocolError::InsufficientData { needed: 6, got: 4 })
        ));
    }

    #[test]
    fn parse_wifi_config_missing_auth() {
        // [ssid_len=3][ssid="Net"][pass_len=4][pass="pass"] - missing auth byte
        let bytes = [3u8, b'N', b'e', b't', 4, b'p', b'a', b's', b's'];
        let result = WifiConfig::from_bytes(&bytes);
        assert!(matches!(
            result,
            Err(ProtocolError::InsufficientData { needed: _, got: _ })
        ));
    }

    #[test]
    fn parse_wifi_config_unknown_auth() {
        // [ssid_len=3][ssid="Net"][pass_len=4][pass="pass"][auth=0x03]
        let bytes = [3u8, b'N', b'e', b't', 4, b'p', b'a', b's', b's', 0x03];
        let result = WifiConfig::from_bytes(&bytes);
        assert!(matches!(result, Err(ProtocolError::UnknownAuthMode(0x03))));
    }

    // WiFi status encoding tests
    #[test]
    fn encode_wifi_disconnected() {
        let status = WifiStatus::disconnected();
        assert_eq!(status.encode(), vec![0x00, 0x00]);
    }

    #[test]
    fn encode_wifi_connecting() {
        let status = WifiStatus::connecting();
        assert_eq!(status.encode(), vec![0x01, 0x00]);
    }

    #[test]
    fn encode_wifi_connected() {
        let status = WifiStatus::connected();
        assert_eq!(status.encode(), vec![0x02, 0x00]);
    }

    #[test]
    fn encode_wifi_failed() {
        let status = WifiStatus::failed("timeout");
        let encoded = status.encode();
        assert_eq!(encoded[0], 0x03);
        assert_eq!(encoded[1], 7); // "timeout" is 7 bytes
        assert_eq!(&encoded[2..], b"timeout");
    }
}
