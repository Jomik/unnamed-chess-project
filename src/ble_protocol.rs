use shakmaty::Color;

use crate::board_api;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("unknown player type byte: 0x{0:02x}")]
    UnknownPlayerType(u8),
    #[error("insufficient data: need {needed} bytes, got {got}")]
    InsufficientData { needed: usize, got: usize },
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

// ---------------------------------------------------------------------------
// PlayerType encoding
// ---------------------------------------------------------------------------

/// Encode a [`board_api::PlayerType`] to its wire byte.
pub fn encode_player_type(pt: board_api::PlayerType) -> u8 {
    match pt {
        board_api::PlayerType::Human => 0x00,
        board_api::PlayerType::Remote => 0x01,
    }
}

/// Decode a wire byte into a [`board_api::PlayerType`].
pub fn decode_player_type(byte: u8) -> Result<board_api::PlayerType, ProtocolError> {
    match byte {
        0x00 => Ok(board_api::PlayerType::Human),
        0x01 => Ok(board_api::PlayerType::Remote),
        other => Err(ProtocolError::UnknownPlayerType(other)),
    }
}

// ---------------------------------------------------------------------------
// Color helpers
// ---------------------------------------------------------------------------

/// Parse a color byte: `0x00` = white, `0x01` = black.
pub fn parse_color(byte: u8) -> Result<Color, ProtocolError> {
    match byte {
        0x00 => Ok(Color::White),
        0x01 => Ok(Color::Black),
        other => Err(ProtocolError::UnknownColor(other)),
    }
}

/// Encode a color: white = `0x00`, black = `0x01`.
pub fn encode_color(color: Color) -> u8 {
    match color {
        Color::White => 0x00,
        Color::Black => 0x01,
    }
}

// ---------------------------------------------------------------------------
// GameStatus encoding
// ---------------------------------------------------------------------------

/// Encode a [`board_api::GameStatus`] to its wire bytes.
///
/// Wire format:
/// - `[0x00]`           – Idle
/// - `[0x01]`           – AwaitingPieces
/// - `[0x02]`           – InProgress
/// - `[0x03, color]`    – Checkmate (color = loser)
/// - `[0x04]`           – Stalemate
/// - `[0x05, color]`    – Resigned (color = resigning side)
pub fn encode_game_status(status: &board_api::GameStatus) -> Vec<u8> {
    match status {
        board_api::GameStatus::Idle => vec![0x00],
        board_api::GameStatus::AwaitingPieces => vec![0x01],
        board_api::GameStatus::InProgress => vec![0x02],
        board_api::GameStatus::Checkmate { loser } => vec![0x03, encode_color(*loser)],
        board_api::GameStatus::Stalemate => vec![0x04],
        board_api::GameStatus::Resigned { color } => vec![0x05, encode_color(*color)],
    }
}

// ---------------------------------------------------------------------------
// BleCommand
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BleCommand {
    StartGame {
        white: board_api::PlayerType,
        black: board_api::PlayerType,
    },
    CancelGame,
    SubmitMove {
        uci: String,
    },
    Resign {
        color: Color,
    },
}

impl BleCommand {
    /// Parse a Start Game characteristic write.
    ///
    /// Format: `[white: u8, black: u8]`
    pub fn parse_start_game(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < 2 {
            return Err(ProtocolError::InsufficientData {
                needed: 2,
                got: bytes.len(),
            });
        }
        let white = decode_player_type(bytes[0])?;
        let black = decode_player_type(bytes[1])?;
        Ok(BleCommand::StartGame { white, black })
    }

    /// Parse a Submit Move characteristic write.
    ///
    /// Format: `[len: u8, uci_bytes...]`
    /// Rejects empty UCI strings (len = 0).
    pub fn parse_submit_move(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let (uci, _) = read_length_prefixed_string(bytes, 0)?;
        if uci.is_empty() {
            return Err(ProtocolError::InsufficientData { needed: 1, got: 0 });
        }
        Ok(BleCommand::SubmitMove { uci })
    }

    /// Parse a Match Control characteristic write.
    ///
    /// Format: `[action: u8, ...]`
    /// - action `0x00` = resign → `[0x00, color: u8]`
    /// - action `0x01` = cancel game → `[0x01]`
    pub fn parse_match_control(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.is_empty() {
            return Err(ProtocolError::InsufficientData { needed: 1, got: 0 });
        }
        let action = bytes[0];
        match action {
            0x00 => {
                // Resign: [0x00, color: u8]
                if bytes.len() < 2 {
                    return Err(ProtocolError::InsufficientData {
                        needed: 2,
                        got: bytes.len(),
                    });
                }
                let color = parse_color(bytes[1])?;
                Ok(BleCommand::Resign { color })
            }
            0x01 => Ok(BleCommand::CancelGame),
            other => Err(ProtocolError::UnknownAction(other)),
        }
    }
}

// ---------------------------------------------------------------------------
// CommandResult
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandSource {
    StartGame = 0x00,
    MatchControl = 0x01,
    SubmitMove = 0x02,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ErrorCode {
    GameAlreadyInProgress = 0x00,
    NoGameInProgress = 0x01,
    NotYourTurn = 0x02,
    IllegalMove = 0x03,
    CannotResignForRemotePlayer = 0x04,
    InvalidCommand = 0x05,
}

/// The result of processing a BLE command.
///
/// Wire encoding: `[ok: u8, source: u8, error_code: u8]`
/// - `ok` is `0x00` for success, `0x01` for error.
/// - `source` identifies which command produced this result.
/// - `error_code` is `0x00` on success (ignored), or one of [`ErrorCode`] on error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub ok: bool,
    pub source: CommandSource,
    pub error_code: Option<ErrorCode>,
}

impl CommandResult {
    pub fn success(source: CommandSource) -> Self {
        Self {
            ok: true,
            source,
            error_code: None,
        }
    }

    pub fn error(source: CommandSource, code: ErrorCode) -> Self {
        Self {
            ok: false,
            source,
            error_code: Some(code),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        if self.ok {
            vec![0x00, self.source as u8, 0x00]
        } else {
            vec![
                0x01,
                self.source as u8,
                self.error_code.unwrap_or(ErrorCode::InvalidCommand) as u8,
            ]
        }
    }
}

// ---------------------------------------------------------------------------
// Move encoding helpers
// ---------------------------------------------------------------------------

/// Encode a move to its wire bytes.
///
/// Format: `[color: u8, uci_len: u8, uci_bytes...]`
pub fn encode_move(color: Color, uci: &str) -> Vec<u8> {
    let uci_bytes = uci.as_bytes();
    let mut out = Vec::with_capacity(2 + uci_bytes.len());
    out.push(encode_color(color));
    out.push(uci_bytes.len() as u8);
    out.extend_from_slice(uci_bytes);
    out
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read a length-prefixed string from `bytes` starting at `offset`.
/// Returns the decoded string and the offset past its end.
fn read_length_prefixed_string(
    bytes: &[u8],
    offset: usize,
) -> Result<(String, usize), ProtocolError> {
    if bytes.len() < offset + 1 {
        return Err(ProtocolError::InsufficientData {
            needed: offset + 1,
            got: bytes.len(),
        });
    }
    let len = bytes[offset] as usize;
    let start = offset + 1;
    let end = start + len;
    if bytes.len() < end {
        return Err(ProtocolError::InsufficientData {
            needed: end,
            got: bytes.len(),
        });
    }
    Ok((
        String::from_utf8_lossy(&bytes[start..end]).into_owned(),
        end,
    ))
}

// ---------------------------------------------------------------------------
// UUIDs
// ---------------------------------------------------------------------------

/// BLE GATT UUID constants.
///
/// All UUIDs share the base `3d6343a2-xxxx-44ea-8fc2-3568d7216866`.
/// They are assigned once and must not change — bonded iOS devices
/// cache discovered services.
pub mod uuids {
    pub const GAME_SERVICE: &str = "3d6343a2-1010-44ea-8fc2-3568d7216866";
    pub const WHITE_PLAYER: &str = "3d6343a2-1011-44ea-8fc2-3568d7216866";
    pub const BLACK_PLAYER: &str = "3d6343a2-1012-44ea-8fc2-3568d7216866";
    pub const START_GAME: &str = "3d6343a2-1013-44ea-8fc2-3568d7216866";
    pub const MATCH_CONTROL: &str = "3d6343a2-1014-44ea-8fc2-3568d7216866";
    pub const GAME_STATUS: &str = "3d6343a2-1015-44ea-8fc2-3568d7216866";
    pub const COMMAND_RESULT: &str = "3d6343a2-1016-44ea-8fc2-3568d7216866";
    pub const SUBMIT_MOVE: &str = "3d6343a2-1017-44ea-8fc2-3568d7216866";
    pub const POSITION: &str = "3d6343a2-1018-44ea-8fc2-3568d7216866";
    pub const LAST_MOVE: &str = "3d6343a2-1019-44ea-8fc2-3568d7216866";
    pub const MOVE_PLAYED: &str = "3d6343a2-101a-44ea-8fc2-3568d7216866";
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use shakmaty::Color;

    // --- encode_player_type / decode_player_type ---

    #[test]
    fn encode_human_player_type() {
        assert_eq!(encode_player_type(board_api::PlayerType::Human), 0x00);
    }

    #[test]
    fn encode_remote_player_type() {
        assert_eq!(encode_player_type(board_api::PlayerType::Remote), 0x01);
    }

    #[test]
    fn decode_human_player_type() {
        assert_eq!(decode_player_type(0x00), Ok(board_api::PlayerType::Human));
    }

    #[test]
    fn decode_remote_player_type() {
        assert_eq!(decode_player_type(0x01), Ok(board_api::PlayerType::Remote));
    }

    #[test]
    fn decode_unknown_player_type() {
        assert!(matches!(
            decode_player_type(0x42),
            Err(ProtocolError::UnknownPlayerType(0x42))
        ));
    }

    #[test]
    fn decode_unset_byte_player_type() {
        assert!(matches!(
            decode_player_type(UNSET_BYTE),
            Err(ProtocolError::UnknownPlayerType(0xFF))
        ));
    }

    #[test]
    fn player_type_encode_decode_roundtrip() {
        for pt in [board_api::PlayerType::Human, board_api::PlayerType::Remote] {
            let encoded = encode_player_type(pt);
            let decoded = decode_player_type(encoded).expect("roundtrip should succeed");
            assert_eq!(decoded, pt);
        }
    }

    // --- encode_color / parse_color ---

    #[test]
    fn encode_color_white() {
        assert_eq!(encode_color(Color::White), 0x00);
    }

    #[test]
    fn encode_color_black() {
        assert_eq!(encode_color(Color::Black), 0x01);
    }

    #[test]
    fn parse_color_white() {
        assert_eq!(parse_color(0x00), Ok(Color::White));
    }

    #[test]
    fn parse_color_black() {
        assert_eq!(parse_color(0x01), Ok(Color::Black));
    }

    #[test]
    fn parse_color_unknown() {
        assert!(matches!(
            parse_color(0x02),
            Err(ProtocolError::UnknownColor(0x02))
        ));
    }

    #[test]
    fn color_encode_parse_roundtrip() {
        for color in [Color::White, Color::Black] {
            let encoded = encode_color(color);
            let decoded = parse_color(encoded).expect("roundtrip should succeed");
            assert_eq!(decoded, color);
        }
    }

    // --- encode_game_status ---

    #[test]
    fn encode_game_status_idle() {
        assert_eq!(encode_game_status(&board_api::GameStatus::Idle), vec![0x00]);
    }

    #[test]
    fn encode_game_status_awaiting_pieces() {
        assert_eq!(
            encode_game_status(&board_api::GameStatus::AwaitingPieces),
            vec![0x01]
        );
    }

    #[test]
    fn encode_game_status_in_progress() {
        assert_eq!(
            encode_game_status(&board_api::GameStatus::InProgress),
            vec![0x02]
        );
    }

    #[test]
    fn encode_game_status_checkmate_white_loses() {
        assert_eq!(
            encode_game_status(&board_api::GameStatus::Checkmate {
                loser: Color::White
            }),
            vec![0x03, 0x00]
        );
    }

    #[test]
    fn encode_game_status_checkmate_black_loses() {
        assert_eq!(
            encode_game_status(&board_api::GameStatus::Checkmate {
                loser: Color::Black
            }),
            vec![0x03, 0x01]
        );
    }

    #[test]
    fn encode_game_status_stalemate() {
        assert_eq!(
            encode_game_status(&board_api::GameStatus::Stalemate),
            vec![0x04]
        );
    }

    #[test]
    fn encode_game_status_resigned_white() {
        assert_eq!(
            encode_game_status(&board_api::GameStatus::Resigned {
                color: Color::White
            }),
            vec![0x05, 0x00]
        );
    }

    #[test]
    fn encode_game_status_resigned_black() {
        assert_eq!(
            encode_game_status(&board_api::GameStatus::Resigned {
                color: Color::Black
            }),
            vec![0x05, 0x01]
        );
    }

    // --- BleCommand::parse_start_game ---

    #[test]
    fn parse_start_game_human_human() {
        let result = BleCommand::parse_start_game(&[0x00, 0x00]);
        assert_eq!(
            result,
            Ok(BleCommand::StartGame {
                white: board_api::PlayerType::Human,
                black: board_api::PlayerType::Human,
            })
        );
    }

    #[test]
    fn parse_start_game_human_remote() {
        let result = BleCommand::parse_start_game(&[0x00, 0x01]);
        assert_eq!(
            result,
            Ok(BleCommand::StartGame {
                white: board_api::PlayerType::Human,
                black: board_api::PlayerType::Remote,
            })
        );
    }

    #[test]
    fn parse_start_game_remote_human() {
        let result = BleCommand::parse_start_game(&[0x01, 0x00]);
        assert_eq!(
            result,
            Ok(BleCommand::StartGame {
                white: board_api::PlayerType::Remote,
                black: board_api::PlayerType::Human,
            })
        );
    }

    #[test]
    fn parse_start_game_insufficient_data_empty() {
        let result = BleCommand::parse_start_game(&[]);
        assert!(matches!(
            result,
            Err(ProtocolError::InsufficientData { needed: 2, got: 0 })
        ));
    }

    #[test]
    fn parse_start_game_insufficient_data_one_byte() {
        let result = BleCommand::parse_start_game(&[0x00]);
        assert!(matches!(
            result,
            Err(ProtocolError::InsufficientData { needed: 2, got: 1 })
        ));
    }

    #[test]
    fn parse_start_game_unknown_white_player_type() {
        let result = BleCommand::parse_start_game(&[0x42, 0x00]);
        assert!(matches!(
            result,
            Err(ProtocolError::UnknownPlayerType(0x42))
        ));
    }

    #[test]
    fn parse_start_game_unknown_black_player_type() {
        let result = BleCommand::parse_start_game(&[0x00, 0xFF]);
        assert!(matches!(
            result,
            Err(ProtocolError::UnknownPlayerType(0xFF))
        ));
    }

    // --- BleCommand::parse_submit_move ---

    #[test]
    fn parse_submit_move_valid_e2e4() {
        // [len=4, 'e', '2', 'e', '4']
        let bytes = [4u8, b'e', b'2', b'e', b'4'];
        let result = BleCommand::parse_submit_move(&bytes);
        assert_eq!(
            result,
            Ok(BleCommand::SubmitMove {
                uci: "e2e4".to_string()
            })
        );
    }

    #[test]
    fn parse_submit_move_valid_promotion() {
        // [len=5, 'e', '7', 'e', '8', 'q']
        let bytes = [5u8, b'e', b'7', b'e', b'8', b'q'];
        let result = BleCommand::parse_submit_move(&bytes);
        assert_eq!(
            result,
            Ok(BleCommand::SubmitMove {
                uci: "e7e8q".to_string()
            })
        );
    }

    #[test]
    fn parse_submit_move_empty_uci_rejected() {
        // [len=0] — empty UCI
        let bytes = [0u8];
        let result = BleCommand::parse_submit_move(&bytes);
        assert!(matches!(
            result,
            Err(ProtocolError::InsufficientData { .. })
        ));
    }

    #[test]
    fn parse_submit_move_no_data() {
        let result = BleCommand::parse_submit_move(&[]);
        assert!(matches!(
            result,
            Err(ProtocolError::InsufficientData { needed: 1, got: 0 })
        ));
    }

    #[test]
    fn parse_submit_move_truncated() {
        // [len=6, 'e', '2', 'e'] — claims 6 bytes but only 3 follow
        let bytes = [6u8, b'e', b'2', b'e'];
        let result = BleCommand::parse_submit_move(&bytes);
        assert!(matches!(
            result,
            Err(ProtocolError::InsufficientData { needed: 7, got: 4 })
        ));
    }

    // --- BleCommand::parse_match_control ---

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
    fn parse_cancel_game() {
        let result = BleCommand::parse_match_control(&[0x01]);
        assert_eq!(result, Ok(BleCommand::CancelGame));
    }

    #[test]
    fn parse_cancel_game_with_extra_bytes() {
        // Extra bytes after action byte should be ignored
        let result = BleCommand::parse_match_control(&[0x01, 0xFF]);
        assert_eq!(result, Ok(BleCommand::CancelGame));
    }

    #[test]
    fn reject_unknown_action() {
        let result = BleCommand::parse_match_control(&[0x02, 0x00]);
        assert!(matches!(result, Err(ProtocolError::UnknownAction(0x02))));
    }

    #[test]
    fn reject_unknown_color_in_match_control() {
        let result = BleCommand::parse_match_control(&[0x00, 0x02]);
        assert!(matches!(result, Err(ProtocolError::UnknownColor(0x02))));
    }

    #[test]
    fn reject_match_control_insufficient_data_resign() {
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
            Err(ProtocolError::InsufficientData { needed: 1, got: 0 })
        ));
    }

    // --- CommandResult encoding ---

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
    fn encode_success_submit_move() {
        let result = CommandResult::success(CommandSource::SubmitMove);
        assert_eq!(result.encode(), vec![0x00, 0x02, 0x00]);
    }

    #[test]
    fn encode_error_game_already_in_progress() {
        let result =
            CommandResult::error(CommandSource::StartGame, ErrorCode::GameAlreadyInProgress);
        assert_eq!(result.encode(), vec![0x01, 0x00, 0x00]);
    }

    #[test]
    fn encode_error_no_game_in_progress() {
        let result = CommandResult::error(CommandSource::MatchControl, ErrorCode::NoGameInProgress);
        assert_eq!(result.encode(), vec![0x01, 0x01, 0x01]);
    }

    #[test]
    fn encode_error_not_your_turn() {
        let result = CommandResult::error(CommandSource::SubmitMove, ErrorCode::NotYourTurn);
        assert_eq!(result.encode(), vec![0x01, 0x02, 0x02]);
    }

    #[test]
    fn encode_error_illegal_move() {
        let result = CommandResult::error(CommandSource::SubmitMove, ErrorCode::IllegalMove);
        assert_eq!(result.encode(), vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn encode_error_cannot_resign_for_remote() {
        let result = CommandResult::error(
            CommandSource::MatchControl,
            ErrorCode::CannotResignForRemotePlayer,
        );
        assert_eq!(result.encode(), vec![0x01, 0x01, 0x04]);
    }

    #[test]
    fn encode_error_invalid_command() {
        let result = CommandResult::error(CommandSource::StartGame, ErrorCode::InvalidCommand);
        assert_eq!(result.encode(), vec![0x01, 0x00, 0x05]);
    }

    #[test]
    fn command_result_ok_field_set_correctly() {
        let ok = CommandResult::success(CommandSource::StartGame);
        assert!(ok.ok);
        assert_eq!(ok.error_code, None);

        let err = CommandResult::error(CommandSource::StartGame, ErrorCode::GameAlreadyInProgress);
        assert!(!err.ok);
        assert_eq!(err.error_code, Some(ErrorCode::GameAlreadyInProgress));
    }

    // --- encode_move ---

    #[test]
    fn encode_move_e2e4_white() {
        let encoded = encode_move(Color::White, "e2e4");
        assert_eq!(encoded, vec![0x00, 4, b'e', b'2', b'e', b'4']);
    }

    #[test]
    fn encode_move_d7d5_black() {
        let encoded = encode_move(Color::Black, "d7d5");
        assert_eq!(encoded, vec![0x01, 4, b'd', b'7', b'd', b'5']);
    }

    #[test]
    fn encode_move_promotion() {
        let encoded = encode_move(Color::White, "e7e8q");
        assert_eq!(encoded, vec![0x00, 5, b'e', b'7', b'e', b'8', b'q']);
    }

    // --- UUID correctness ---

    #[test]
    fn uuid_game_service() {
        assert_eq!(uuids::GAME_SERVICE, "3d6343a2-1010-44ea-8fc2-3568d7216866");
    }

    #[test]
    fn uuid_white_player() {
        assert_eq!(uuids::WHITE_PLAYER, "3d6343a2-1011-44ea-8fc2-3568d7216866");
    }

    #[test]
    fn uuid_black_player() {
        assert_eq!(uuids::BLACK_PLAYER, "3d6343a2-1012-44ea-8fc2-3568d7216866");
    }

    #[test]
    fn uuid_start_game() {
        assert_eq!(uuids::START_GAME, "3d6343a2-1013-44ea-8fc2-3568d7216866");
    }

    #[test]
    fn uuid_match_control() {
        assert_eq!(uuids::MATCH_CONTROL, "3d6343a2-1014-44ea-8fc2-3568d7216866");
    }

    #[test]
    fn uuid_game_status() {
        assert_eq!(uuids::GAME_STATUS, "3d6343a2-1015-44ea-8fc2-3568d7216866");
    }

    #[test]
    fn uuid_command_result() {
        assert_eq!(
            uuids::COMMAND_RESULT,
            "3d6343a2-1016-44ea-8fc2-3568d7216866"
        );
    }

    #[test]
    fn uuid_submit_move() {
        assert_eq!(uuids::SUBMIT_MOVE, "3d6343a2-1017-44ea-8fc2-3568d7216866");
    }

    #[test]
    fn uuid_position() {
        assert_eq!(uuids::POSITION, "3d6343a2-1018-44ea-8fc2-3568d7216866");
    }

    #[test]
    fn uuid_last_move() {
        assert_eq!(uuids::LAST_MOVE, "3d6343a2-1019-44ea-8fc2-3568d7216866");
    }

    #[test]
    fn uuid_move_played() {
        assert_eq!(uuids::MOVE_PLAYED, "3d6343a2-101a-44ea-8fc2-3568d7216866");
    }
}
