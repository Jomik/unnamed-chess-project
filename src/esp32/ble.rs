use std::sync::mpsc;

use esp32_nimble::utilities::mutex::Mutex;
use esp32_nimble::{
    BLEAdvertisementData, BLECharacteristic, BLEDevice, BLEServer, NimbleProperties,
    utilities::BleUuid, uuid128,
};
use shakmaty::Color;

use crate::ble_protocol::{
    BleCommand, CommandResult, CommandSource, GameState, PlayerConfig, UNSET_BYTE, uuids,
};

use alloc::sync::Arc;
extern crate alloc;

type ChrHandle = Arc<Mutex<BLECharacteristic>>;

#[derive(Debug, thiserror::Error)]
pub enum BleError {
    #[error("NimBLE error: {0}")]
    Nimble(#[from] esp32_nimble::BLEError),
}

const ATT_ERR_REQUEST_NOT_SUPPORTED: u8 = 0x06;

struct GameStateHandle(ChrHandle);

impl GameStateHandle {
    fn notify(&self, state: &GameState) {
        let encoded = state.encode();
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

struct PlayerConfigHandle(ChrHandle);

impl PlayerConfigHandle {
    fn update(&self, bytes: &[u8]) {
        self.0.lock().set_value(bytes);
    }
}

struct GameHandles {
    white_player: PlayerConfigHandle,
    black_player: PlayerConfigHandle,
    game_state: GameStateHandle,
    command_result: CommandResultHandle,
}

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
    game_state: GameStateHandle,
    command_result: CommandResultHandle,
    white_player: PlayerConfigHandle,
    black_player: PlayerConfigHandle,
}

impl std::fmt::Debug for BleNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BleNotifier").finish_non_exhaustive()
    }
}

impl BleNotifier {
    pub fn notify_game_state(&self, state: &GameState) {
        self.game_state.notify(state);
    }

    pub fn notify_command_result(&self, result: &CommandResult) {
        self.command_result.notify(result);
    }

    pub fn update_player_config(&self, color: Color, bytes: &[u8]) {
        match color {
            Color::White => self.white_player.update(bytes),
            Color::Black => self.black_player.update(bytes),
        }
    }
}

/// Returns [`BleCommands`] (inbound) and [`BleNotifier`] (outbound).
pub fn start_ble() -> Result<(BleCommands, BleNotifier), BleError> {
    let device = BLEDevice::take();

    let (tx, rx) = mpsc::sync_channel::<BleCommand>(8);

    let server = device.get_server();

    register_stub_service(
        server,
        uuid128!(uuids::WIFI_SERVICE),
        uuid128!(uuids::WIFI_CONFIG),
        uuid128!(uuids::WIFI_STATUS),
    );

    register_stub_service(
        server,
        uuid128!(uuids::LICHESS_SERVICE),
        uuid128!(uuids::LICHESS_TOKEN),
        uuid128!(uuids::LICHESS_STATUS),
    );

    let GameHandles {
        white_player,
        black_player,
        game_state,
        command_result,
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
            game_state,
            command_result,
            white_player,
            black_player,
        },
    ))
}

/// WiFi and Lichess stubs -- must exist in GATT table for forward compatibility.
fn register_stub_service(
    server: &mut BLEServer,
    service_uuid: BleUuid,
    config_uuid: BleUuid,
    status_uuid: BleUuid,
) {
    let svc = server.create_service(service_uuid);
    let mut svc = svc.lock();

    let config_chr = svc.create_characteristic(config_uuid, NimbleProperties::WRITE);
    config_chr.lock().on_write(|args| {
        args.reject_with_error_code(ATT_ERR_REQUEST_NOT_SUPPORTED);
    });

    let _status_chr = svc.create_characteristic(
        status_uuid,
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );
}

fn register_player_characteristic(
    svc: &mut impl std::ops::DerefMut<Target = esp32_nimble::BLEService>,
    uuid: BleUuid,
    tx: &mpsc::SyncSender<BleCommand>,
    make_command: fn(PlayerConfig) -> BleCommand,
    label: &str,
) -> PlayerConfigHandle {
    let player_chr =
        svc.create_characteristic(uuid, NimbleProperties::READ | NimbleProperties::WRITE);
    {
        let tx = tx.clone();
        let label = label.to_string();
        player_chr
            .lock()
            .set_value(&[UNSET_BYTE])
            .on_write(
                move |args| match PlayerConfig::from_bytes(args.recv_data()) {
                    Ok(config) => {
                        if let Err(e) = tx.try_send(make_command(config)) {
                            log::warn!("BLE command channel full ({}): {e}", label);
                        }
                    }
                    Err(e) => {
                        log::warn!("Invalid {} Player write: {e}", label);
                        args.reject_with_error_code(ATT_ERR_REQUEST_NOT_SUPPORTED);
                    }
                },
            );
    }
    PlayerConfigHandle(player_chr)
}

fn register_game_service(server: &mut BLEServer, tx: &mpsc::SyncSender<BleCommand>) -> GameHandles {
    let game_svc = server.create_service(uuid128!(uuids::GAME_SERVICE));
    let mut svc = game_svc.lock();

    let white_player = register_player_characteristic(
        &mut svc,
        uuid128!(uuids::WHITE_PLAYER),
        tx,
        BleCommand::SetWhitePlayer,
        "white",
    );

    let black_player = register_player_characteristic(
        &mut svc,
        uuid128!(uuids::BLACK_PLAYER),
        tx,
        BleCommand::SetBlackPlayer,
        "black",
    );

    let start_game_chr =
        svc.create_characteristic(uuid128!(uuids::START_GAME), NimbleProperties::WRITE);
    {
        let tx = tx.clone();
        start_game_chr.lock().on_write(move |_args| {
            if let Err(e) = tx.try_send(BleCommand::StartGame) {
                log::warn!("BLE command channel full (start): {e}");
            }
        });
    }

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

    let game_state_chr = svc.create_characteristic(
        uuid128!(uuids::GAME_STATE),
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );
    game_state_chr.lock().set_value(&GameState::idle().encode());

    let command_result_chr = svc.create_characteristic(
        uuid128!(uuids::COMMAND_RESULT),
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );
    command_result_chr
        .lock()
        .set_value(&CommandResult::success(CommandSource::StartGame).encode());

    GameHandles {
        white_player,
        black_player,
        game_state: GameStateHandle(game_state_chr),
        command_result: CommandResultHandle(command_result_chr),
    }
}
