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
        XCTAssertEqual(PlayerType.lichessAi.encode(level: 1).count, 2)
    }

    func testEncodeLichessAiLevel1() {
        XCTAssertEqual(
            PlayerType.lichessAi.encode(level: 1),
            Data([0x02, 0x01])
        )
    }

    func testEncodeLichessAiLevel8() {
        XCTAssertEqual(
            PlayerType.lichessAi.encode(level: 8),
            Data([0x02, 0x08])
        )
    }

    func testEncodeLichessAiLevel4() {
        XCTAssertEqual(
            PlayerType.lichessAi.encode(level: 4),
            Data([0x02, 0x04])
        )
    }
}
