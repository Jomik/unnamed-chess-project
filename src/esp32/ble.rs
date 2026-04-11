use std::sync::mpsc;

use esp32_nimble::utilities::mutex::Mutex;
use esp32_nimble::{
    BLEAdvertisementData, BLECharacteristic, BLEDevice, BLEServer, NimbleProperties, uuid128,
};
use shakmaty::Color;

use crate::ble_protocol::{self, BleCommand, CommandResult, CommandSource, UNSET_BYTE, uuids};
use crate::board_api;

use alloc::sync::Arc;
extern crate alloc;

type ChrHandle = Arc<Mutex<BLECharacteristic>>;

#[derive(Debug, thiserror::Error)]
pub enum BleError {
    #[error("NimBLE error: {0}")]
    Nimble(#[from] esp32_nimble::BLEError),
}

const ATT_ERR_REQUEST_NOT_SUPPORTED: u8 = 0x06;

// ---------------------------------------------------------------------------
// Handle wrappers
// ---------------------------------------------------------------------------

struct GameStatusHandle(ChrHandle);

impl GameStatusHandle {
    fn notify(&self, status: &board_api::GameStatus) {
        let encoded = ble_protocol::encode_game_status(status);
        self.0.lock().set_value(&encoded).notify();
    }
}

/// Cheap to clone (wraps an `Arc`); the clone is used in the on-connect
/// callback so it can reset the characteristic without needing a shared ref.
#[derive(Clone)]
struct CommandResultHandle(ChrHandle);

impl CommandResultHandle {
    fn notify(&self, result: &CommandResult) {
        let encoded = result.encode();
        self.0.lock().set_value(&encoded).notify();
    }

    /// Called on every BLE reconnect per protocol spec.
    fn reset(&self) {
        self.notify(&CommandResult::success(CommandSource::StartGame));
    }
}

struct PlayerTypeHandle(ChrHandle);

impl PlayerTypeHandle {
    fn update(&self, pt: board_api::PlayerType) {
        let byte = ble_protocol::encode_player_type(pt);
        self.0.lock().set_value(&[byte]).notify();
    }

    fn reset(&self) {
        self.0.lock().set_value(&[UNSET_BYTE]).notify();
    }
}

struct PositionHandle(ChrHandle);

impl PositionHandle {
    fn update(&self, fen: &str) {
        self.0.lock().set_value(fen.as_bytes()).notify();
    }

    fn reset(&self) {
        self.0.lock().set_value(&[]).notify();
    }
}

struct LastMoveHandle(ChrHandle);

impl LastMoveHandle {
    fn update(&self, color: Color, uci: &str) {
        let encoded = ble_protocol::encode_move(color, uci);
        self.0.lock().set_value(&encoded).notify();
    }

    fn reset(&self) {
        self.0.lock().set_value(&[]).notify();
    }
}

struct MovePlayedHandle(ChrHandle);

impl MovePlayedHandle {
    fn notify(&self, color: Color, uci: &str) {
        let encoded = ble_protocol::encode_move(color, uci);
        self.0.lock().set_value(&encoded).notify();
    }
}

// ---------------------------------------------------------------------------
// GameHandles — internal bundle returned from register_game_service
// ---------------------------------------------------------------------------

struct GameHandles {
    white_player: PlayerTypeHandle,
    black_player: PlayerTypeHandle,
    game_status: GameStatusHandle,
    command_result: CommandResultHandle,
    position: PositionHandle,
    last_move: LastMoveHandle,
    move_played: MovePlayedHandle,
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct BleCommands {
    rx: mpsc::Receiver<BleCommand>,
}

impl std::fmt::Debug for BleCommands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BleCommands").finish_non_exhaustive()
    }
}

impl BleCommands {
    pub fn try_recv(&self) -> Option<BleCommand> {
        self.rx.try_recv().ok()
    }
}

pub struct BleNotifier {
    game_status: GameStatusHandle,
    command_result: CommandResultHandle,
    white_player: PlayerTypeHandle,
    black_player: PlayerTypeHandle,
    position: PositionHandle,
    last_move: LastMoveHandle,
    move_played: MovePlayedHandle,
}

impl std::fmt::Debug for BleNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BleNotifier").finish_non_exhaustive()
    }
}

impl BleNotifier {
    /// Update the game status characteristic and notify subscribers.
    pub fn notify_game_status(&self, status: &board_api::GameStatus) {
        self.game_status.notify(status);
    }

    /// Notify the command result characteristic.
    pub fn notify_command_result(&self, result: &CommandResult) {
        self.command_result.notify(result);
    }

    /// Update a player type characteristic (read+notify) after a game starts.
    pub fn update_player_type(&self, color: Color, pt: board_api::PlayerType) {
        match color {
            Color::White => self.white_player.update(pt),
            Color::Black => self.black_player.update(pt),
        }
    }

    /// Notify the MovePlayed characteristic (notify-only) when a move is played.
    pub fn notify_move_played(&self, color: Color, uci: &str) {
        self.move_played.notify(color, uci);
    }

    /// Update the Position (FEN) characteristic and notify subscribers.
    pub fn update_position(&self, fen: &str) {
        self.position.update(fen);
    }

    /// Update the LastMove characteristic and notify subscribers.
    pub fn update_last_move(&self, color: Color, uci: &str) {
        self.last_move.update(color, uci);
    }

    /// Reset both player type characteristics to UNSET_BYTE (called after game ends).
    pub fn reset_player_types(&self) {
        self.white_player.reset();
        self.black_player.reset();
    }

    /// Clear the Position characteristic (called when game ends).
    pub fn reset_position(&self) {
        self.position.reset();
    }

    /// Clear the LastMove characteristic (called when game ends).
    pub fn reset_last_move(&self) {
        self.last_move.reset();
    }
}

// ---------------------------------------------------------------------------
// start_ble
// ---------------------------------------------------------------------------

/// Returns [`BleCommands`] (inbound) and [`BleNotifier`] (outbound).
pub fn start_ble() -> Result<(BleCommands, BleNotifier), BleError> {
    let device = BLEDevice::take();

    let (tx, rx) = mpsc::sync_channel::<BleCommand>(8);

    let server = device.get_server();

    let GameHandles {
        white_player,
        black_player,
        game_status,
        command_result,
        position,
        last_move,
        move_played,
    } = register_game_service(server, &tx);

    {
        let cmd_result = command_result.clone();
        server.on_connect(move |_server, desc| {
            log::info!("BLE client connected: {:?}", desc);
            cmd_result.reset();
        });
    }

    server.on_disconnect(|_desc, reason| {
        log::info!("BLE client disconnected ({:?})", reason);
    });

    let advertising = device.get_advertising();
    advertising.lock().set_data(
        BLEAdvertisementData::new()
            .name("ChessBoard")
            .add_service_uuid(uuid128!(uuids::GAME_SERVICE)),
    )?;
    advertising.lock().start()?;

    log::info!("BLE server started");

    Ok((
        BleCommands { rx },
        BleNotifier {
            game_status,
            command_result,
            white_player,
            black_player,
            position,
            last_move,
            move_played,
        },
    ))
}

// ---------------------------------------------------------------------------
// register_game_service
// ---------------------------------------------------------------------------

fn register_game_service(server: &mut BLEServer, tx: &mpsc::SyncSender<BleCommand>) -> GameHandles {
    let game_svc = server.create_service(uuid128!(uuids::GAME_SERVICE));
    let mut svc = game_svc.lock();

    // White Player — read+notify; updated by firmware after StartGame is processed.
    let white_player_chr = svc.create_characteristic(
        uuid128!(uuids::WHITE_PLAYER),
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );
    white_player_chr.lock().set_value(&[UNSET_BYTE]);

    // Black Player — read+notify; updated by firmware after StartGame is processed.
    let black_player_chr = svc.create_characteristic(
        uuid128!(uuids::BLACK_PLAYER),
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );
    black_player_chr.lock().set_value(&[UNSET_BYTE]);

    // Start Game — write; carries [white_type: u8, black_type: u8].
    let start_game_chr =
        svc.create_characteristic(uuid128!(uuids::START_GAME), NimbleProperties::WRITE);
    {
        let tx = tx.clone();
        start_game_chr.lock().on_write(move |args| {
            match BleCommand::parse_start_game(args.recv_data()) {
                Ok(cmd) => {
                    if let Err(e) = tx.try_send(cmd) {
                        log::warn!("BLE command channel full (start_game): {e}");
                    }
                }
                Err(e) => {
                    log::warn!("Invalid Start Game write: {e}");
                    args.reject_with_error_code(ATT_ERR_REQUEST_NOT_SUPPORTED);
                }
            }
        });
    }

    // Match Control — write; carries resign / cancel actions.
    let match_control_chr =
        svc.create_characteristic(uuid128!(uuids::MATCH_CONTROL), NimbleProperties::WRITE);
    {
        let tx = tx.clone();
        match_control_chr.lock().on_write(move |args| {
            match BleCommand::parse_match_control(args.recv_data()) {
                Ok(cmd) => {
                    if let Err(e) = tx.try_send(cmd) {
                        log::warn!("BLE command channel full (match_control): {e}");
                    }
                }
                Err(e) => {
                    log::warn!("Invalid Match Control write: {e}");
                    args.reject_with_error_code(ATT_ERR_REQUEST_NOT_SUPPORTED);
                }
            }
        });
    }

    // Submit Move — write; carries [len: u8, uci_bytes...].
    let submit_move_chr =
        svc.create_characteristic(uuid128!(uuids::SUBMIT_MOVE), NimbleProperties::WRITE);
    {
        let tx = tx.clone();
        submit_move_chr.lock().on_write(move |args| {
            match BleCommand::parse_submit_move(args.recv_data()) {
                Ok(cmd) => {
                    if let Err(e) = tx.try_send(cmd) {
                        log::warn!("BLE command channel full (submit_move): {e}");
                    }
                }
                Err(e) => {
                    log::warn!("Invalid Submit Move write: {e}");
                    args.reject_with_error_code(ATT_ERR_REQUEST_NOT_SUPPORTED);
                }
            }
        });
    }

    // Game Status — read+notify; encoded GameStatus bytes.
    let game_status_chr = svc.create_characteristic(
        uuid128!(uuids::GAME_STATUS),
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );
    game_status_chr
        .lock()
        .set_value(&ble_protocol::encode_game_status(
            &board_api::GameStatus::Idle,
        ));

    // Command Result — read+notify; encoded CommandResult bytes.
    let command_result_chr = svc.create_characteristic(
        uuid128!(uuids::COMMAND_RESULT),
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );
    command_result_chr
        .lock()
        .set_value(&CommandResult::success(CommandSource::StartGame).encode());

    // Position — read+notify; FEN string bytes (empty until game starts).
    let position_chr = svc.create_characteristic(
        uuid128!(uuids::POSITION),
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );
    position_chr.lock().set_value(&[]);

    // Last Move — read+notify; encoded move bytes (empty until first move).
    let last_move_chr = svc.create_characteristic(
        uuid128!(uuids::LAST_MOVE),
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );
    last_move_chr.lock().set_value(&[]);

    // Move Played — notify-only; fired each time a move is committed.
    let move_played_chr =
        svc.create_characteristic(uuid128!(uuids::MOVE_PLAYED), NimbleProperties::NOTIFY);

    GameHandles {
        white_player: PlayerTypeHandle(white_player_chr),
        black_player: PlayerTypeHandle(black_player_chr),
        game_status: GameStatusHandle(game_status_chr),
        command_result: CommandResultHandle(command_result_chr),
        position: PositionHandle(position_chr),
        last_move: LastMoveHandle(last_move_chr),
        move_played: MovePlayedHandle(move_played_chr),
    }
}
