import XCTest

@testable import ChessBoard

final class PlayerTypeTests: XCTestCase {
    func testEncodeHuman() {
        XCTAssertEqual(PlayerType.human.encode(), Data([0x00]))
    }

    func testEncodeEmbedded() {
        XCTAssertEqual(PlayerType.embedded.encode(), Data([0x01]))
    }

    func testEncodedLengths() {
        XCTAssertEqual(PlayerType.human.encode().count, 1)
        XCTAssertEqual(PlayerType.embedded.encode().count, 1)
    }
}
