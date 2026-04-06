import Foundation

/// Player type for a side. Mirrors firmware `PlayerConfig`.
///
/// Wire format (tagged binary):
/// - Human:      `[0x00]`
/// - Embedded:   `[0x01]`
/// - Lichess AI: `[0x02, level: u8]`
///
/// Level is a separate parameter to `encode(level:)` rather than an
/// associated value, because SwiftUI Picker bindings require a simple
/// Equatable tag.
enum PlayerType: UInt8, Equatable {
    case human = 0x00
    case embedded = 0x01
    case lichessAi = 0x02

    func encode(level: Int = 0) -> Data {
        switch self {
        case .human: return Data([0x00])
        case .embedded: return Data([0x01])
        case .lichessAi: return Data([0x02, UInt8(level)])
        }
    }
}
