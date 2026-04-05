import CoreBluetooth
import Observation

@Observable
class LichessManager {
    var status: LichessStatus = .idle

    private weak var peripheral: CBPeripheral?
    private var tokenChar: CBCharacteristic?
    private var statusChar: CBCharacteristic?

    func attach(to peripheral: CBPeripheral) {
        self.peripheral = peripheral
    }

    func setToken(_ token: String) {
        guard let peripheral, let tokenChar else { return }
        let tokenBytes = Array(token.utf8)
        guard tokenBytes.count <= 255 else { return }
        var data = Data([UInt8(tokenBytes.count)])
        data.append(contentsOf: tokenBytes)
        peripheral.writeValue(
            data,
            for: tokenChar,
            type: .withResponse
        )
    }

    func discoverCharacteristics(for service: CBService) {
        guard let chars = service.characteristics else { return }
        for char in chars {
            switch char.uuid {
            case GATT.lichessToken:
                tokenChar = char
            case GATT.lichessStatus:
                statusChar = char
                peripheral?.setNotifyValue(true, for: char)
                peripheral?.readValue(for: char)
            default:
                break
            }
        }
    }

    func handleNotification(_ data: Data) {
        if let decoded = LichessStatus.decode(data) {
            status = decoded
        }
    }

    func handleWriteError() {
        status = LichessStatus(
            state: .failed,
            message: "Write failed"
        )
    }

    func reset() {
        peripheral = nil
        tokenChar = nil
        statusChar = nil
        status = .idle
    }
}
