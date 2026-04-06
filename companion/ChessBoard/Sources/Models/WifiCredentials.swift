import Foundation

/// WiFi credentials for Keychain persistence.
struct WifiCredentials: Codable {
    let ssid: String
    let password: String
    let authMode: WifiAuthMode
}
