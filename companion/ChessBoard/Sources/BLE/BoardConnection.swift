import CoreBluetooth
import Observation

enum ConnectionState: Equatable {
    case poweredOff
    case scanning
    case connecting
    case discoveringServices
    case ready
}

@Observable
class BoardConnection {
    var connectionState: ConnectionState = .poweredOff
    var gameState: GameState = .initial
    var lastCommandResult: CommandResult?

    /// Player types written by the app when starting a game.
    /// Used to determine which side is human for resign.
    private(set) var whitePlayerType: PlayerType?
    private(set) var blackPlayerType: PlayerType?

    let wifiManager = WifiManager()
    let lichessManager = LichessManager()

    private let delegate: BLEDelegate

    init() {
        delegate = BLEDelegate()
        delegate.owner = self
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

        delegate.write(
            white.encode(level: whiteLevel),
            to: GATT.whitePlayer
        )
        delegate.write(
            black.encode(level: blackLevel),
            to: GATT.blackPlayer
        )
        delegate.write(Data(), to: GATT.startGame)
    }

    func resign(color: Turn) {
        lastCommandResult = nil
        // Match Control: [action=resign(0x00), color]
        delegate.write(Data([0x00, color.rawValue]), to: GATT.matchControl)
    }
}

private class BLEDelegate: NSObject {
    weak var owner: BoardConnection?

    private var centralManager: CBCentralManager!
    private var peripheral: CBPeripheral?
    private var characteristics: [CBUUID: CBCharacteristic] = [:]
    private var awaitingInitialState = false

    override init() {
        super.init()
        // queue: nil → main queue. This is load-bearing for @Observable.
        centralManager = CBCentralManager(delegate: self, queue: nil)
    }

    func write(_ data: Data, to uuid: CBUUID) {
        guard let peripheral, let char = characteristics[uuid] else { return }
        peripheral.writeValue(data, for: char, type: .withResponse)
    }

    private func startScanning() {
        guard centralManager.state == .poweredOn else { return }
        owner?.connectionState = .scanning
        centralManager.scanForPeripherals(
            withServices: [GATT.gameService],
            options: nil
        )
    }
}

extension BLEDelegate: CBCentralManagerDelegate {
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        switch central.state {
        case .poweredOn:
            startScanning()
        default:
            owner?.connectionState = .poweredOff
        }
    }

    func centralManager(
        _ central: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi RSSI: NSNumber
    ) {
        central.stopScan()
        self.peripheral = peripheral
        peripheral.delegate = self
        central.connect(peripheral, options: nil)
        owner?.connectionState = .connecting
    }

    func centralManager(
        _ central: CBCentralManager,
        didConnect peripheral: CBPeripheral
    ) {
        owner?.connectionState = .discoveringServices
        peripheral.discoverServices(GATT.allServices)
        owner?.wifiManager.attach(to: peripheral)
        owner?.lichessManager.attach(to: peripheral)
    }

    func centralManager(
        _ central: CBCentralManager,
        didFailToConnect peripheral: CBPeripheral,
        error: Error?
    ) {
        startScanning()
    }

    func centralManager(
        _ central: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        characteristics.removeAll()
        awaitingInitialState = false
        owner?.wifiManager.reset()
        owner?.lichessManager.reset()
        owner?.connectionState = .connecting
        central.connect(peripheral, options: nil)
    }
}

extension BLEDelegate: CBPeripheralDelegate {
    func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverServices error: Error?
    ) {
        guard let services = peripheral.services else { return }
        for service in services {
            switch service.uuid {
            case GATT.gameService:
                peripheral.discoverCharacteristics(
                    GATT.gameCharacteristics,
                    for: service
                )
            case GATT.wifiService:
                peripheral.discoverCharacteristics(
                    GATT.wifiCharacteristics,
                    for: service
                )
            case GATT.lichessService:
                peripheral.discoverCharacteristics(
                    GATT.lichessCharacteristics,
                    for: service
                )
            default:
                break
            }
        }
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        switch service.uuid {
        case GATT.gameService:
            guard let chars = service.characteristics else {
                return
            }
            for char in chars {
                characteristics[char.uuid] = char
                if char.uuid == GATT.gameState
                    || char.uuid == GATT.commandResult
                {
                    peripheral.setNotifyValue(true, for: char)
                }
            }
            if let gs = characteristics[GATT.gameState],
                !awaitingInitialState,
                owner?.connectionState != .ready
            {
                awaitingInitialState = true
                peripheral.readValue(for: gs)
            }
        case GATT.wifiService:
            owner?.wifiManager.discoverCharacteristics(
                for: service
            )
        case GATT.lichessService:
            owner?.lichessManager.discoverCharacteristics(
                for: service
            )
        default:
            break
        }
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        if error != nil {
            if awaitingInitialState {
                awaitingInitialState = false
                owner?.connectionState = .ready
            }
            return
        }
        guard let data = characteristic.value else { return }
        switch characteristic.uuid {
        case GATT.gameState:
            if let state = GameState.decode(data) {
                owner?.gameState = state
            }
            // Transition to .ready after we have actual game state
            if awaitingInitialState {
                awaitingInitialState = false
                owner?.connectionState = .ready
            }
        case GATT.commandResult:
            if let result = CommandResult.decode(data) {
                owner?.lastCommandResult = result
            }
        case GATT.wifiStatus:
            owner?.wifiManager.handleNotification(data)
        case GATT.lichessStatus:
            owner?.lichessManager.handleNotification(data)
        default:
            break
        }
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didWriteValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        if error == nil { return }
        switch characteristic.uuid {
        case GATT.wifiConfig:
            owner?.wifiManager.handleWriteError()
        case GATT.lichessToken:
            owner?.lichessManager.handleWriteError()
        default:
            break
        }
    }
}
