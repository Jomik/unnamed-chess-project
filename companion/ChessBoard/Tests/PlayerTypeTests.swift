import XCTest

@testable import ChessBoard

final class PlayerTypeTests: XCTestCase {
    func testEncodeHuman() {
        XCTAssertEqual(PlayerType.human.encode(), Data([0x00]))
    }

    func testEncodeRemote() {
        XCTAssertEqual(PlayerType.remote.encode(), Data([0x01]))
    }

    func testEncodedLengths() {
        XCTAssertEqual(PlayerType.human.encode().count, 1)
        XCTAssertEqual(PlayerType.remote.encode().count, 1)
    }

    func testDecodeHuman() {
        XCTAssertEqual(PlayerType.decode(Data([0x00])), .human)
    }

    func testDecodeRemote() {
        XCTAssertEqual(PlayerType.decode(Data([0x01])), .remote)
    }

    func testDecodeUnknown() {
        XCTAssertNil(PlayerType.decode(Data([0xFF])))
    }

    func testDecodeEmpty() {
        XCTAssertNil(PlayerType.decode(Data()))
    }
}
