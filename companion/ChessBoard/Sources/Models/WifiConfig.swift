import Foundation

/// WiFi auth mode. Mirrors firmware `WifiAuthMode` repr(u8).
enum WifiAuthMode: UInt8, Equatable, Codable, CaseIterable {
    case open = 0x00
    case wpa2 = 0x01
    case wpa3 = 0x02
}

/// WiFi credentials sent to the board.
///
/// Wire format: `[auth_mode: u8, ssid_len: u8, ssid..., pass_len: u8, pass...]`
struct WifiConfig {
    let ssid: String
    let password: String
    let authMode: WifiAuthMode

    func encode() -> Data {
        var data = Data([authMode.rawValue])
        let ssidBytes = Array(ssid.utf8)
        data.append(UInt8(ssidBytes.count))
        data.append(contentsOf: ssidBytes)
        let passBytes = Array(password.utf8)
        data.append(UInt8(passBytes.count))
        data.append(contentsOf: passBytes)
        return data
    }
}
