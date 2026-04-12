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

    /// Current board position as FEN string, updated from position characteristic.
    var currentPosition: String?

    /// Last move played: (color: Turn, uci: String).
    var lastMove: (color: Turn, uci: String)?

    /// Called when the board emits a MovePlayed notification.
    var onMovePlayed: ((Turn, String) -> Void)?

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
            currentPosition: String? = nil,
            lastCommandResult: CommandResult? = nil,
            whitePlayerType: PlayerType? = .human,
            blackPlayerType: PlayerType? = .remote
        ) {
            self.transport = nil
            self.connectionState = connectionState
            self.gameStatus = gameStatus
            self.currentPosition = currentPosition
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
    /// In human-vs-human, the side whose turn it is.
    var resignColor: Turn? {
        switch (whitePlayerType, blackPlayerType) {
        case (.human, .human): return activeTurn
        case (.human, _): return .white
        case (_, .human): return .black
        default: return nil
        }
    }

    /// Derives the active color from the current FEN position.
    private var activeTurn: Turn? {
        guard let fen = currentPosition else { return nil }
        let components = fen.split(separator: " ")
        guard components.count >= 2 else { return nil }
        return components[1] == "w" ? .white : .black
    }

    func configureAndStart(
        white: PlayerType,
        black: PlayerType
    ) {
        guard connectionState == .ready else { return }

        whitePlayerType = white
        blackPlayerType = black
        lastCommandResult = nil

        transport?.write(
            Data([white.rawValue, black.rawValue]),
            to: GATT.startGame
        )
    }

    /// Sends a resign command via Match Control.
    ///
    /// Wire format: `[action: u8 (0x00 = resign), color: u8]`
    func resign(color: Turn) {
        lastCommandResult = nil
        transport?.write(Data([0x00, color.rawValue]), to: GATT.matchControl)
    }

    /// Sends a cancel/abort command via Match Control.
    ///
    /// Wire format: `[action: u8 (0x01 = cancel)]`
    func cancelGame() {
        lastCommandResult = nil
        transport?.write(Data([0x01]), to: GATT.matchControl)
    }

    /// Sends a move to the board.
    ///
    /// Wire format: `[length: u8, ...uci_bytes]`
    func submitMove(_ uci: String) {
        let bytes = Array(uci.utf8)
        guard bytes.count <= 255 else { return }
        lastCommandResult = nil
        var data = Data([UInt8(bytes.count)])
        data.append(contentsOf: bytes)
        transport?.write(data, to: GATT.submitMove)
    }

    /// Handles a move played notification from the board.
    func handleMovePlayed(color: Turn, uci: String) {
        lastMove = (color, uci)
        onMovePlayed?(color, uci)
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
