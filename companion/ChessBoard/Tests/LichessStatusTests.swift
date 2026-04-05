import XCTest

@testable import ChessBoard

final class LichessStatusTests: XCTestCase {
    func testDecodeIdle() {
        let status = LichessStatus.decode(Data([0x00, 0x00]))
        XCTAssertEqual(
            status,
            LichessStatus(state: .idle, message: "")
        )
    }

    func testDecodeValidating() {
        let status = LichessStatus.decode(Data([0x01, 0x00]))
        XCTAssertEqual(
            status,
            LichessStatus(state: .validating, message: "")
        )
    }

    func testDecodeConnected() {
        let status = LichessStatus.decode(Data([0x02, 0x00]))
        XCTAssertEqual(
            status,
            LichessStatus(state: .connected, message: "")
        )
    }

    func testDecodeFailedWithMessage() {
        let msg = "invalid token"
        let data =
            Data([0x03, UInt8(msg.utf8.count)])
            + msg.data(using: .utf8)!
        let status = LichessStatus.decode(data)
        XCTAssertEqual(
            status,
            LichessStatus(state: .failed, message: "invalid token")
        )
    }

    func testDecodeFailedEmptyMessage() {
        let status = LichessStatus.decode(Data([0x03, 0x00]))
        XCTAssertEqual(
            status,
            LichessStatus(state: .failed, message: "")
        )
    }

    func testDecodeInvalidState() {
        XCTAssertNil(LichessStatus.decode(Data([0xFF, 0x00])))
    }

    func testDecodeTooShort() {
        XCTAssertNil(LichessStatus.decode(Data([0x00])))
        XCTAssertNil(LichessStatus.decode(Data()))
    }

    func testDecodeTruncatedMessage() {
        XCTAssertNil(
            LichessStatus.decode(Data([0x03, 0x05, 0x68, 0x69]))
        )
    }

    func testIdleConstant() {
        XCTAssertEqual(
            LichessStatus.idle,
            LichessStatus(state: .idle, message: "")
        )
    }
}
