import Foundation
import Testing

@testable import ChessBoard

@Suite struct PlayerTypeTests {
    @Test func encodeHuman() {
        #expect(PlayerType.human.encode() == Data([0x00]))
    }

    @Test func encodeRemote() {
        #expect(PlayerType.remote.encode() == Data([0x01]))
    }

    @Test func encodedLengths() {
        #expect(PlayerType.human.encode().count == 1)
        #expect(PlayerType.remote.encode().count == 1)
    }

    @Test func decodeHuman() {
        #expect(PlayerType.decode(Data([0x00])) == .human)
    }

    @Test func decodeRemote() {
        #expect(PlayerType.decode(Data([0x01])) == .remote)
    }

    @Test func decodeUnknown() {
        #expect(PlayerType.decode(Data([0xFF])) == nil)
    }

    @Test func decodeEmpty() {
        #expect(PlayerType.decode(Data()) == nil)
    }
}
