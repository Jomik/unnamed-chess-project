import CoreBluetooth
import Observation

enum ConnectionState: Equatable {
    case poweredOff
    case scanning
    case notFound
    case connecting
    case connectionFailed
    case discoveringServices
    case setupFailed
    case ready
}

extension ConnectionState {
    var isTimeoutable: Bool {
        switch self {
        case .scanning, .connecting, .discoveringServices:
            return true
        default:
            return false
        }
    }

    var isFailure: Bool {
        switch self {
        case .notFound, .connectionFailed, .setupFailed:
            return true
        default:
            return false
        }
    }
}

@MainActor
@Observable
class BoardConnection {
    var connectionState: ConnectionState = .poweredOff
    var gameState: GameState = .initial
    var lastCommandResult: CommandResult?
    var wifiStatus: WifiStatus = .disconnected
    var lichessStatus: LichessStatus = .idle

    /// Player types written by the app when starting a game.
    /// Used to determine which side is human for resign.
    private(set) var whitePlayerType: PlayerType?
    private(set) var blackPlayerType: PlayerType?

    private var transport: BoardTransport?

    init() {
        let ble = BLETransport()
        self.transport = ble
        ble.owner = self
    }

    /// Testing initializer — accepts any BoardTransport.
    init(transport: BoardTransport) {
        self.transport = transport
        transport.owner = self
    }

    /// The human player's color (nil if both or neither are human).
    var humanColor: Turn? {
        switch (whitePlayerType, blackPlayerType) {
        case (.human, .human): return nil
        case (.human, _): return .white
        case (_, .human): return .black
        default: return nil
        }
    }

    /// Color to resign for. In human-vs-engine, always the human side.
    /// In human-vs-human, the side whose turn it is.
    var resignColor: Turn? {
        switch (whitePlayerType, blackPlayerType) {
        case (.human, .human): return gameState.turn
        case (.human, _): return .white
        case (_, .human): return .black
        default: return nil
        }
    }

    func configureWifi(ssid: String, password: String, authMode: WifiAuthMode) {
        wifiStatus = WifiStatus(state: .connecting, message: "")
        let config = WifiConfig(
            ssid: ssid,
            password: password,
            authMode: authMode
        )
        transport?.write(config.encode(), to: GATT.wifiConfig)
    }

    func setLichessToken(_ token: String) {
        let tokenBytes = Array(token.utf8)
        guard tokenBytes.count <= 255 else { return }
        lichessStatus = LichessStatus(state: .validating, message: "")
        var data = Data([UInt8(tokenBytes.count)])
        data.append(contentsOf: tokenBytes)
        transport?.write(data, to: GATT.lichessToken)
    }

    func configureAndStart(
        white: PlayerType,
        whiteLevel: Int = 0,
        black: PlayerType,
        blackLevel: Int = 0
    ) {
        guard connectionState == .ready else { return }

        whitePlayerType = white
        blackPlayerType = black
        lastCommandResult = nil

        transport?.write(
            white.encode(level: whiteLevel),
            to: GATT.whitePlayer
        )
        transport?.write(
            black.encode(level: blackLevel),
            to: GATT.blackPlayer
        )
        transport?.write(Data(), to: GATT.startGame)
    }

    /// Sends a resign command via Match Control.
    ///
    /// Wire format: `[action: u8 (0x00 = resign), color: u8]`
    func resign(color: Turn) {
        lastCommandResult = nil
        transport?.write(Data([0x00, color.rawValue]), to: GATT.matchControl)
    }

    func restartScanning() {
        transport?.restartScanning()
    }

    func connectionTimedOut() {
        switch connectionState {
        case .scanning:
            transport?.stopScanning()
            connectionState = .notFound
        case .connecting:
            transport?.cancelConnection()
            connectionState = .connectionFailed
        case .discoveringServices:
            transport?.cancelConnection()
            connectionState = .setupFailed
        default:
            break
        }
    }
}
