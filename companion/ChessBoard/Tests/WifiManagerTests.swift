import XCTest

@testable import ChessBoard

final class WifiManagerTests: XCTestCase {
    var manager: WifiManager!

    override func setUp() {
        super.setUp()
        manager = WifiManager()
    }

    override func tearDown() {
        manager = nil
        super.tearDown()
    }

    func testInitialStatus() {
        XCTAssertEqual(
            manager.status,
            WifiStatus.disconnected
        )
    }

    func testHandleNotificationConnected() {
        let data = Data([0x02, 0x00])
        manager.handleNotification(data)
        XCTAssertEqual(
            manager.status,
            WifiStatus(state: .connected, message: "")
        )
    }

    func testHandleNotificationFailed() {
        let msg = "timeout"
        let data =
            Data([0x03, UInt8(msg.utf8.count)])
            + msg.data(using: .utf8)!
        manager.handleNotification(data)
        XCTAssertEqual(
            manager.status,
            WifiStatus(state: .failed, message: "timeout")
        )
    }

    func testHandleNotificationInvalidData() {
        manager.status = WifiStatus(
            state: .connected,
            message: ""
        )
        let invalidData = Data([0xFF])
        manager.handleNotification(invalidData)
        // Status should remain unchanged on invalid decode
        XCTAssertEqual(
            manager.status,
            WifiStatus(state: .connected, message: "")
        )
    }

    func testHandleWriteError() {
        manager.status = WifiStatus(
            state: .connected,
            message: ""
        )
        manager.handleWriteError()
        XCTAssertEqual(
            manager.status,
            WifiStatus(state: .failed, message: "Write failed")
        )
    }

    func testReset() {
        manager.status = WifiStatus(
            state: .connected,
            message: "connected"
        )
        manager.reset()
        XCTAssertEqual(
            manager.status,
            WifiStatus.disconnected
        )
    }
}
