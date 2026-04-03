import Foundation

/// Player type for a side. Mirrors firmware `PlayerConfig`.
///
/// Wire format (tagged binary):
/// - Human:    `[0x00]`
/// - Embedded: `[0x01]`
enum PlayerType: Equatable {
    case human
    case embedded

    func encode() -> Data {
        switch self {
        case .human: return Data([0x00])
        case .embedded: return Data([0x01])
        }
    }
}
