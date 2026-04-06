import CoreBluetooth
import Observation

@Observable
class WifiManager {
    var status: WifiStatus = .disconnected

    private weak var peripheral: CBPeripheral?
    private var configChar: CBCharacteristic?
    private var statusChar: CBCharacteristic?

    func attach(to peripheral: CBPeripheral) {
        self.peripheral = peripheral
    }

    func configure(
        ssid: String,
        password: String,
        authMode: WifiAuthMode
    ) {
        guard let peripheral, let configChar else { return }
        let config = WifiConfig(
            ssid: ssid,
            password: password,
            authMode: authMode
        )
        peripheral.writeValue(
            config.encode(),
            for: configChar,
            type: .withResponse
        )
    }

    func discoverCharacteristics(for service: CBService) {
        guard let chars = service.characteristics else { return }
        for char in chars {
            switch char.uuid {
            case GATT.wifiConfig:
                configChar = char
            case GATT.wifiStatus:
                statusChar = char
                peripheral?.setNotifyValue(true, for: char)
                peripheral?.readValue(for: char)
            default:
                break
            }
        }
    }

    func handleNotification(_ data: Data) {
        if let decoded = WifiStatus.decode(data) {
            status = decoded
        }
    }

    func handleWriteError() {
        status = WifiStatus(
            state: .failed,
            message: "Write failed"
        )
    }

    func reset() {
        peripheral = nil
        configChar = nil
        statusChar = nil
        status = .disconnected
    }
}
