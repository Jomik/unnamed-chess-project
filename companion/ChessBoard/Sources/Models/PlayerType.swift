import Foundation

enum PlayerType: UInt8, Equatable {
    case human = 0x00
    case remote = 0x01

    func encode() -> Data { Data([rawValue]) }

    static func decode(_ data: Data) -> PlayerType? {
        guard let first = data.first else { return nil }
        return PlayerType(rawValue: first)
    }
}
