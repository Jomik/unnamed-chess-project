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
    var gameStatus: GameStatus = .idle
    var lastCommandResult: CommandResult?

    /// Player types for each side, read from firmware on connect.
    /// Used to determine which side is human for resign.
    var whitePlayerType: PlayerType?
    var blackPlayerType: PlayerType?

    private var transport: BoardTransport?

    /// Production initializer. Callers must provide the transport explicitly.
    init(transport: BoardTransport) {
        self.transport = transport
        transport.owner = self
    }

    #if DEBUG
        /// Creates a BoardConnection with pre-set state and no BLE transport.
        /// Commands are no-ops (transport is nil). For use in #Preview macros.
        init(
            connectionState: ConnectionState = .ready,
            gameStatus: GameStatus = .idle,
            lastCommandResult: CommandResult? = nil,
            whitePlayerType: PlayerType? = .human,
            blackPlayerType: PlayerType? = .remote
        ) {
            self.transport = nil
            self.connectionState = connectionState
            self.gameStatus = gameStatus
            self.lastCommandResult = lastCommandResult
            self.whitePlayerType = whitePlayerType
            self.blackPlayerType = blackPlayerType
        }
    #endif

    /// The human player's color (nil if both or neither are human).
    var humanColor: Turn? {
        switch (whitePlayerType, blackPlayerType) {
        case (.human, .human): return nil
        case (.human, _): return .white
        case (_, .human): return .black
        default: return nil
        }
    }

    /// Color to resign for. In human-vs-remote, always the human side.
    /// In human-vs-human, returns nil until color selection is implemented.
    var resignColor: Turn? {
        switch (whitePlayerType, blackPlayerType) {
        case (.human, .human): return nil
        case (.human, _): return .white
        case (_, .human): return .black
        default: return nil
        }
    }

    func configureAndStart(
        white: PlayerType,
        black: PlayerType
    ) {
        guard connectionState == .ready else { return }

        whitePlayerType = white
        blackPlayerType = black
        lastCommandResult = nil

        transport?.write(white.encode(), to: GATT.whitePlayer)
        transport?.write(black.encode(), to: GATT.blackPlayer)
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
