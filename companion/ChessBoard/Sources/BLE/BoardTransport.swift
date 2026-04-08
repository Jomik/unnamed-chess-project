import CoreBluetooth

/// Abstraction over the BLE transport layer.
///
/// The transport handles scanning, connecting, service/characteristic
/// discovery, and raw writes. It calls back to BoardConnection via the
/// weak `owner` reference when state changes occur.
@MainActor
protocol BoardTransport: AnyObject {
    /// Weak back-reference to the owning BoardConnection.
    /// Set immediately after init by BoardConnection.
    var owner: BoardConnection? { get set }

    /// Write raw data to the characteristic identified by the given GATT UUID.
    func write(_ data: Data, to characteristic: CBUUID)

    /// Stop current scan, clear peripheral state, and start a fresh scan.
    func restartScanning()

    /// Stop an in-progress scan (called on timeout).
    func stopScanning()

    /// Cancel the current peripheral connection (called on timeout).
    func cancelConnection()
}
