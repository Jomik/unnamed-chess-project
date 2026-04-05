import XCTest

@testable import ChessBoard

final class WifiStatusTests: XCTestCase {
    func testDecodeDisconnected() {
        let status = WifiStatus.decode(Data([0x00, 0x00]))
        XCTAssertEqual(
            status,
            WifiStatus(state: .disconnected, message: "")
        )
    }

    func testDecodeConnecting() {
        let status = WifiStatus.decode(Data([0x01, 0x00]))
        XCTAssertEqual(
            status,
            WifiStatus(state: .connecting, message: "")
        )
    }

    func testDecodeConnected() {
        let status = WifiStatus.decode(Data([0x02, 0x00]))
        XCTAssertEqual(
            status,
            WifiStatus(state: .connected, message: "")
        )
    }

    func testDecodeFailedWithMessage() {
        let msg = "timeout"
        let data =
            Data([0x03, UInt8(msg.utf8.count)])
            + msg.data(using: .utf8)!
        let status = WifiStatus.decode(data)
        XCTAssertEqual(
            status,
            WifiStatus(state: .failed, message: "timeout")
        )
    }

    func testDecodeFailedEmptyMessage() {
        let status = WifiStatus.decode(Data([0x03, 0x00]))
        XCTAssertEqual(
            status,
            WifiStatus(state: .failed, message: "")
        )
    }

    func testDecodeInvalidState() {
        XCTAssertNil(WifiStatus.decode(Data([0xFF, 0x00])))
    }

    func testDecodeTooShort() {
        XCTAssertNil(WifiStatus.decode(Data([0x00])))
        XCTAssertNil(WifiStatus.decode(Data()))
    }

    func testDecodeTruncatedMessage() {
        XCTAssertNil(
            WifiStatus.decode(Data([0x03, 0x05, 0x68, 0x69]))
        )
    }

    func testDisconnectedConstant() {
        XCTAssertEqual(
            WifiStatus.disconnected,
            WifiStatus(state: .disconnected, message: "")
        )
    }
}
