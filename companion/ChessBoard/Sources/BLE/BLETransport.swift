import CoreBluetooth

final class BLETransport: NSObject, BoardTransport,
    CBCentralManagerDelegate, CBPeripheralDelegate
{
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

    func restartScanning() {
        centralManager.stopScan()
        peripheral = nil
        characteristics.removeAll()
        startScanning()
    }

    func stopScanning() {
        centralManager.stopScan()
    }

    func cancelConnection() {
        guard let p = peripheral else { return }
        // Clear state BEFORE asking CoreBluetooth to cancel, guaranteeing
        // the delegate guards (didDisconnect / didFailToConnect) will see
        // self.peripheral == nil and bail out.
        peripheral = nil
        characteristics.removeAll()
        centralManager.cancelPeripheralConnection(p)
    }

    private func startScanning() {
        guard centralManager.state == .poweredOn else { return }
        owner?.connectionState = .scanning
        centralManager.scanForPeripherals(
            withServices: [GATT.gameService],
            options: nil
        )
    }

    // MARK: - CBCentralManagerDelegate

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
        rssi rssiValue: NSNumber
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
    }

    func centralManager(
        _ central: CBCentralManager,
        didFailToConnect peripheral: CBPeripheral,
        error: Error?
    ) {
        // cancelConnection() nils self.peripheral before this callback fires.
        // If nil, the cancellation was intentional — don't restart scanning.
        guard self.peripheral != nil else { return }
        startScanning()
    }

    func centralManager(
        _ central: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        // cancelConnection() nils self.peripheral before this callback fires.
        // If nil, the disconnect was intentional — don't auto-reconnect.
        guard self.peripheral != nil else { return }
        characteristics.removeAll()
        awaitingInitialState = false
        owner?.connectionState = .connecting
        central.connect(peripheral, options: nil)
    }

    // MARK: - CBPeripheralDelegate

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
                // Subscribe to notifications for status and move events
                if char.uuid == GATT.gameStatus
                    || char.uuid == GATT.commandResult
                    || char.uuid == GATT.position
                    || char.uuid == GATT.lastMove
                    || char.uuid == GATT.movePlayed
                {
                    peripheral.setNotifyValue(true, for: char)
                }
            }
            // Read current game state once after discovery to seed the UI before transitioning to .ready.
            if let gs = characteristics[GATT.gameStatus],
                !awaitingInitialState,
                owner?.connectionState != .ready
            {
                awaitingInitialState = true
                peripheral.readValue(for: gs)
            }
            if let wp = characteristics[GATT.whitePlayer] {
                peripheral.readValue(for: wp)
            }
            if let bp = characteristics[GATT.blackPlayer] {
                peripheral.readValue(for: bp)
            }
            if let pos = characteristics[GATT.position] {
                peripheral.readValue(for: pos)
            }
            if let lm = characteristics[GATT.lastMove] {
                peripheral.readValue(for: lm)
            }
        default:
            break
        }
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        // If the initial game-state read fails, transition to .ready anyway;
        // the board will push state via notifications once a game starts.
        if error != nil {
            if awaitingInitialState {
                awaitingInitialState = false
                owner?.connectionState = .ready
            }
            return
        }
        guard let data = characteristic.value else { return }
        switch characteristic.uuid {
        case GATT.gameStatus:
            if let status = GameStatus.decode(data) {
                owner?.gameStatus = status
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
        case GATT.whitePlayer:
            owner?.whitePlayerType = PlayerType.decode(data)
        case GATT.blackPlayer:
            owner?.blackPlayerType = PlayerType.decode(data)
        case GATT.position:
            if data.isEmpty {
                owner?.currentPosition = nil
            } else if let positionString = String(data: data, encoding: .utf8) {
                owner?.currentPosition = positionString
            }
        case GATT.lastMove:
            if data.isEmpty {
                owner?.lastMove = nil
            } else {
                decodeAndHandleMove(
                    data: data,
                    handler: { color, uci in
                        owner?.lastMove = (color, uci)
                    }
                )
            }
        case GATT.movePlayed:
            decodeAndHandleMove(
                data: data,
                handler: { color, uci in
                    owner?.handleMovePlayed(color: color, uci: uci)
                }
            )
        default:
            break
        }
    }

    private func decodeAndHandleMove(
        data: Data,
        handler: (Turn, String) -> Void
    ) {
        guard data.count >= 3,
            let colorByte = data.first,
            let color = Turn(rawValue: colorByte),
            let uciLen = data.dropFirst().first,
            data.count >= 2 + Int(uciLen)
        else { return }
        let uciBytes = data.dropFirst(2).prefix(Int(uciLen))
        if let uciString = String(bytes: uciBytes, encoding: .utf8) {
            handler(color, uciString)
        }
    }

    func peripheral(
        _ peripheral: CBPeripheral,
        didWriteValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        // No write-error handling needed for the new protocol.
    }
}
