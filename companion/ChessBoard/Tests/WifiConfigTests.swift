import XCTest

@testable import ChessBoard

final class WifiConfigTests: XCTestCase {
    func testEncodeWpa2() {
        let config = WifiConfig(
            ssid: "MyNet",
            password: "pass123",
            authMode: .wpa2
        )
        let data = config.encode()
        XCTAssertEqual(
            data,
            Data(
                [0x01, 5] + Array("MyNet".utf8) + [7]
                    + Array("pass123".utf8)
            )
        )
    }

    func testEncodeWpa3() {
        let config = WifiConfig(
            ssid: "Network",
            password: "password",
            authMode: .wpa3
        )
        let data = config.encode()
        XCTAssertEqual(
            data,
            Data(
                [0x02, 7] + Array("Network".utf8) + [8]
                    + Array("password".utf8)
            )
        )
    }

    func testEncodeOpenEmptyPassword() {
        let config = WifiConfig(
            ssid: "Open",
            password: "",
            authMode: .open
        )
        let data = config.encode()
        XCTAssertEqual(
            data,
            Data([0x00, 4] + Array("Open".utf8) + [0])
        )
    }

    func testEncodeAuthModeRawValues() {
        XCTAssertEqual(WifiAuthMode.open.rawValue, 0x00)
        XCTAssertEqual(WifiAuthMode.wpa2.rawValue, 0x01)
        XCTAssertEqual(WifiAuthMode.wpa3.rawValue, 0x02)
    }
}
