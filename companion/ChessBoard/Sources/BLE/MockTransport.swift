#if DEBUG
    import CoreBluetooth
    import Foundation

    /// A mock BoardTransport that records all write and lifecycle calls.
    /// Available in DEBUG builds for use in previews and tests.
    @MainActor
    final class MockTransport: BoardTransport {
        weak var owner: BoardConnection?

        // MARK: - write(_:to:)

        var writeCallCount = 0
        var writeArgs: [(data: Data, characteristic: CBUUID)] = []

        func write(_ data: Data, to characteristic: CBUUID) {
            writeCallCount += 1
            writeArgs.append((data, characteristic))
        }

        // MARK: - restartScanning()

        var restartScanningCallCount = 0

        func restartScanning() {
            restartScanningCallCount += 1
        }

        // MARK: - stopScanning()

        var stopScanningCallCount = 0

        func stopScanning() {
            stopScanningCallCount += 1
        }

        // MARK: - cancelConnection()

        var cancelConnectionCallCount = 0

        func cancelConnection() {
            cancelConnectionCallCount += 1
        }
    }
#endif
