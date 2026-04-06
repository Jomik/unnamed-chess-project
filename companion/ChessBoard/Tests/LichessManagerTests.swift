import XCTest

@testable import ChessBoard

final class LichessManagerTests: XCTestCase {
    var manager: LichessManager!

    override func setUp() {
        super.setUp()
        manager = LichessManager()
    }

    override func tearDown() {
        manager = nil
        super.tearDown()
    }

    func testInitialStatus() {
        XCTAssertEqual(manager.status, .idle)
    }

    func testHandleNotificationConnected() {
        manager.handleNotification(Data([0x02, 0x00]))
        XCTAssertEqual(
            manager.status,
            LichessStatus(state: .connected, message: "")
        )
    }

    func testHandleNotificationFailed() {
        let msg = "invalid token"
        let data =
            Data([0x03, UInt8(msg.utf8.count)])
            + msg.data(using: .utf8)!
        manager.handleNotification(data)
        XCTAssertEqual(
            manager.status,
            LichessStatus(
                state: .failed,
                message: "invalid token"
            )
        )
    }

    func testHandleNotificationInvalidData() {
        manager.handleNotification(Data([0xFF]))
        XCTAssertEqual(manager.status, .idle)
    }

    func testHandleWriteError() {
        manager.handleWriteError()
        XCTAssertEqual(manager.status.state, .failed)
        XCTAssertEqual(
            manager.status.message,
            "Write failed"
        )
    }

    func testReset() {
        manager.handleNotification(Data([0x02, 0x00]))
        XCTAssertEqual(manager.status.state, .connected)
        manager.reset()
        XCTAssertEqual(manager.status, .idle)
    }
}
