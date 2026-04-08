# Companion App Preview & Simulator Support Design

## Overview

The companion app cannot render any view in `#Preview` macros or run meaningfully in the iOS Simulator because every view depends on `BoardConnection`, which unconditionally creates a `CBCentralManager` on init. This design extracts a `BoardTransport` protocol from the BLE delegate, folds the WiFi/Lichess manager state into `BoardConnection`, and adds a `#if DEBUG` initializer that skips BLE -- enabling `#Preview` macros and Simulator usage while improving the overall architecture.

## Goals

- All four views (`ContentView`, `ScanView`, `NewGameView`, `ActiveGameView`) render in `#Preview` macros with representative states (scanning, connection failures, ready/idle, in-progress game, terminal states, WiFi connecting/failed, Lichess validating, etc.).
- The app launches in the Simulator with a mock board connection showing `NewGameView` instead of being stuck on `ScanView`.
- Views continue to use `@Environment(BoardConnection.self)` -- no view-layer protocol abstraction, no existential types, no observation-tracking risks.
- The production BLE code path is preserved exactly (BLE starts on init, same timing).
- Existing model tests (`GameStateTests`, `WifiStatusTests`, etc.) continue to pass without modification.

## Non-Goals

- Fully functional BLE simulation (replaying recorded BLE traffic, simulating board responses to commands).
- Unit testing of view logic (e.g., verifying that tapping "Start Game" calls `configureAndStart`).
- Abstracting `KeychainStore` or `UserDefaults` for preview isolation.
- Making `BoardTransport` generic/reusable for other BLE devices -- this is a single-purpose app.

## Architecture

Three structural changes work together:

1. **Extract `BoardTransport` protocol** -- the current private `BLEDelegate` class is extracted into its own file as `BLETransport`, conforming to a `BoardTransport` protocol. This is the single abstraction boundary; it separates CoreBluetooth I/O from application state management.
2. **Fold `WifiManager` and `LichessManager` into `BoardConnection`** -- their observable status properties and command methods move into `BoardConnection` directly. Their CoreBluetooth references (`CBPeripheral`, `CBCharacteristic`) move into `BLETransport`. This eliminates two classes.
3. **Add `#if DEBUG` initializer** -- a second initializer on `BoardConnection` accepts pre-set state and creates no transport. The production `init()` creates a `BLETransport` and starts scanning immediately, preserving current behavior exactly.

```
┌─────────────────────────────────────────────────────┐
│                       Views                          │
│  @Environment(BoardConnection.self) private var board│
│  Read: board.connectionState, board.wifiStatus, etc. │
│  Write: board.configureAndStart(...), board.resign()  │
└────────────────────────┬────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────┐
│                  BoardConnection                     │
│  @Observable class — single source of truth          │
│  Owns all state: connectionState, gameState,         │
│    wifiStatus, lichessStatus, lastCommandResult,     │
│    whitePlayerType, blackPlayerType                  │
│  Commands: configureAndStart, resign, configureWifi, │
│    setLichessToken, restartScanning, connectionTimedOut│
│  Delegates BLE I/O to transport via optional chaining │
└────────────────────────┬────────────────────────────┘
                         │ BoardTransport protocol
┌────────────────────────▼────────────────────────────┐
│  BLETransport (production only)                      │
│  CBCentralManager + CBPeripheral delegates           │
│  Calls back via weak owner: BoardConnection?         │
│  Owns all CBCharacteristic refs                      │
└─────────────────────────────────────────────────────┘
```

## BoardTransport Protocol

Defines the narrow set of operations that vary between real BLE and no-op (preview/simulator). Located in `Sources/BLE/BoardTransport.swift`.

```swift
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
```

The protocol surface is minimal: four operations. Everything else -- state decoding, state management, GATT characteristic routing, notification handling -- lives in `BLETransport` and flows back to `BoardConnection` via the `owner` back-reference.

The transport uses `CBUUID` (from CoreBluetooth) in the `write` signature. `BoardTransport.swift` imports CoreBluetooth for this type. `BoardConnection.swift` also needs `import CoreBluetooth` because it references `GATT.*` constants (which are `CBUUID` values) when calling `transport?.write(data, to: GATT.whitePlayer)`. This is the same import the file already has today.

## BLETransport

The current private `BLEDelegate` class is extracted into `Sources/BLE/BLETransport.swift` as a non-private class conforming to `BoardTransport`, `CBCentralManagerDelegate`, and `CBPeripheralDelegate`.

The implementation is a mechanical extraction of the existing `BLEDelegate` code with two changes:

1. **WiFi/Lichess characteristic discovery and notification routing** -- currently delegated to `WifiManager.discoverCharacteristics(for:)` and `LichessManager.handleNotification(_:)` -- moves inline into `BLETransport`. The transport discovers WiFi/Lichess characteristics, subscribes to status notifications, and routes decoded updates directly to `owner?.wifiStatus` and `owner?.lichessStatus`.

2. **WiFi/Lichess characteristic references** -- currently held by `WifiManager` and `LichessManager` -- move into `BLETransport`'s `characteristics` dictionary (which already stores game service characteristics). All characteristic writes go through the transport's existing `write(_:to:)` method.

The transport remains GATT-aware (it references `GATT.*` constants to know which services to discover, which characteristics to subscribe to, etc.). Adding a new BLE characteristic requires adding the UUID to `GATT.swift` and adding handling in `BLETransport`'s delegate methods -- the same pattern as today.

### Write error handling

The current `BLEDelegate.peripheral(_:didWriteValueFor:error:)` routes write errors to `WifiManager.handleWriteError()` and `LichessManager.handleWriteError()`. With managers folded into `BoardConnection`, `BLETransport` routes write errors directly:

```swift
func peripheral(_ peripheral: CBPeripheral,
                didWriteValueFor characteristic: CBCharacteristic,
                error: Error?) {
    guard error != nil else { return }
    switch characteristic.uuid {
    case GATT.wifiConfig:
        owner?.wifiStatus = WifiStatus(state: .failed, message: "Write failed")
    case GATT.lichessToken:
        owner?.lichessStatus = LichessStatus(state: .failed, message: "Write failed")
    default:
        break
    }
}
```

## BoardConnection

### State consolidation

`WifiManager.status` and `LichessManager.status` become direct properties on `BoardConnection`:

```swift
@Observable
class BoardConnection {
    var connectionState: ConnectionState = .poweredOff
    var gameState: GameState = .initial
    var lastCommandResult: CommandResult?
    var wifiStatus: WifiStatus = .disconnected
    var lichessStatus: LichessStatus = .idle

    private(set) var whitePlayerType: PlayerType?
    private(set) var blackPlayerType: PlayerType?

    private var transport: BoardTransport?

    // ... methods ...
}
```

Views access `board.wifiStatus` instead of `board.wifiManager.status`. This is one level of property access instead of two, which is simpler to reason about. Swift Observation tracks it identically (concrete `@Observable` class, no existentials).

### Command methods

`WifiManager.configure(ssid:password:authMode:)` becomes `BoardConnection.configureWifi(ssid:password:authMode:)`. `LichessManager.setToken(_:)` becomes `BoardConnection.setLichessToken(_:)`. The encode-and-write logic moves inline:

```swift
func configureWifi(ssid: String, password: String, authMode: WifiAuthMode) {
    guard transport != nil else { return }
    wifiStatus = WifiStatus(state: .connecting, message: "")
    let config = WifiConfig(ssid: ssid, password: password, authMode: authMode)
    transport?.write(config.encode(), to: GATT.wifiConfig)
}

func setLichessToken(_ token: String) {
    guard transport != nil else { return }
    let tokenBytes = Array(token.utf8)
    guard tokenBytes.count <= 255 else { return }
    lichessStatus = LichessStatus(state: .validating, message: "")
    var data = Data([UInt8(tokenBytes.count)])
    data.append(contentsOf: tokenBytes)
    transport?.write(data, to: GATT.lichessToken)
}
```

Existing command methods (`configureAndStart`, `resign`, `restartScanning`, `connectionTimedOut`) use optional chaining on `transport` where they previously called `delegate` methods.

### Computed properties

`humanColor` and `resignColor` remain unchanged -- they are pure functions of `whitePlayerType`, `blackPlayerType`, and `gameState.turn`.

### Initializers

```swift
/// Production initializer. Creates BLETransport, which immediately
/// starts CBCentralManager and begins scanning.
init() {
    let ble = BLETransport()
    self.transport = ble
    ble.owner = self
}

#if DEBUG
/// Creates a BoardConnection with pre-set state and no BLE transport.
/// Commands are no-ops (transport is nil). For use in #Preview macros
/// and Simulator builds.
init(
    connectionState: ConnectionState = .ready,
    gameState: GameState = .initial,
    wifiStatus: WifiStatus = .disconnected,
    lichessStatus: LichessStatus = .idle,
    lastCommandResult: CommandResult? = nil,
    whitePlayerType: PlayerType? = .human,
    blackPlayerType: PlayerType? = .embedded
) {
    self.transport = nil
    self.connectionState = connectionState
    self.gameState = gameState
    self.wifiStatus = wifiStatus
    self.lichessStatus = lichessStatus
    self.lastCommandResult = lastCommandResult
    self.whitePlayerType = whitePlayerType
    self.blackPlayerType = blackPlayerType
}
#endif
```

The `#if DEBUG` init provides defaults tuned for the common preview case: `.ready` connection with idle game, human-vs-engine. Every parameter is overridable.

## View Changes

### Environment injection

All views continue to use:

```swift
@Environment(BoardConnection.self) private var board
```

No change. `@Observable` tracking works correctly because views access a concrete `@Observable` class.

### NewGameView

The only view with substantive changes. All references to `board.wifiManager` and `board.lichessManager` are replaced:

| Before | After |
|--------|-------|
| `board.wifiManager.status` | `board.wifiStatus` |
| `board.wifiManager.status.state` | `board.wifiStatus.state` |
| `board.wifiManager.status.message` | `board.wifiStatus.message` |
| `board.wifiManager.configure(ssid:password:authMode:)` | `board.configureWifi(ssid:password:authMode:)` |
| `board.lichessManager.status` | `board.lichessStatus` |
| `board.lichessManager.status.state` | `board.lichessStatus.state` |
| `board.lichessManager.status.message` | `board.lichessStatus.message` |
| `board.lichessManager.setToken(...)` | `board.setLichessToken(...)` |
| `board.lichessManager.status = LichessStatus(...)` | `board.lichessStatus = LichessStatus(...)` |

Additionally, `NewGameView` gets a `#if DEBUG` internal initializer that accepts initial player types. This is needed because the WiFi and Lichess form sections are gated behind a `needsLichess` computed property that checks whether either player is `.lichessAi` -- and those are local `@State` variables that default to `.human` / `.embedded`. Without this init, previews for WiFi/Lichess states would never render the relevant sections.

```swift
#if DEBUG
init(whiteType: PlayerType = .human, blackType: PlayerType = .embedded) {
    _whiteType = State(initialValue: whiteType)
    _blackType = State(initialValue: blackType)
}
#endif
```

The existing parameterless `init` (implicit) remains the production entry point.

### Other views

`ContentView`, `ScanView`, and `ActiveGameView` have no changes to their existing code. They do not reference `wifiManager` or `lichessManager`.

## ChessBoardApp

Simulator gate to use the debug init:

```swift
@main
struct ChessBoardApp: App {
    @State private var board: BoardConnection = {
        #if DEBUG && targetEnvironment(simulator)
        BoardConnection(connectionState: .ready)
        #else
        BoardConnection()
        #endif
    }()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(board)
        }
    }
}
```

The compound condition `DEBUG && targetEnvironment(simulator)` ensures the debug init is only used in debug builds on the Simulator. Release builds on the Simulator (e.g., for profiling) use the production init, and debug builds on a physical device use real BLE.

## Preview Macros

Each view file gets `#Preview` macros at the bottom, inside `#if DEBUG`, covering its key visual states. Previews use the debug init directly:

**ScanView:**

```swift
#if DEBUG
#Preview("Scanning") {
    NavigationStack { ScanView() }
        .environment(BoardConnection(connectionState: .scanning))
}
#Preview("Not Found") {
    NavigationStack { ScanView() }
        .environment(BoardConnection(connectionState: .notFound))
}
#Preview("Connection Failed") {
    NavigationStack { ScanView() }
        .environment(BoardConnection(connectionState: .connectionFailed))
}
#Preview("Setup Failed") {
    NavigationStack { ScanView() }
        .environment(BoardConnection(connectionState: .setupFailed))
}
#Preview("Powered Off") {
    NavigationStack { ScanView() }
        .environment(BoardConnection(connectionState: .poweredOff))
}
#endif
```

**NewGameView:**

```swift
#if DEBUG
#Preview("Idle") {
    NavigationStack { NewGameView() }
        .environment(BoardConnection())
}
#Preview("WiFi Connecting") {
    NavigationStack { NewGameView(whiteType: .human, blackType: .lichessAi) }
        .environment(BoardConnection(
            wifiStatus: WifiStatus(state: .connecting, message: "")
        ))
}
#Preview("WiFi Connected") {
    NavigationStack { NewGameView(whiteType: .human, blackType: .lichessAi) }
        .environment(BoardConnection(
            wifiStatus: WifiStatus(state: .connected, message: "")
        ))
}
#Preview("WiFi Failed") {
    NavigationStack { NewGameView(whiteType: .human, blackType: .lichessAi) }
        .environment(BoardConnection(
            wifiStatus: WifiStatus(state: .failed, message: "Connection timed out")
        ))
}
#Preview("Lichess Validating") {
    NavigationStack { NewGameView(whiteType: .human, blackType: .lichessAi) }
        .environment(BoardConnection(
            wifiStatus: WifiStatus(state: .connected, message: ""),
            lichessStatus: LichessStatus(state: .validating, message: "")
        ))
}
#Preview("Lichess Connected") {
    NavigationStack { NewGameView(whiteType: .human, blackType: .lichessAi) }
        .environment(BoardConnection(
            wifiStatus: WifiStatus(state: .connected, message: ""),
            lichessStatus: LichessStatus(state: .connected, message: "")
        ))
}
#endif
```

**ActiveGameView:**

```swift
#if DEBUG
#Preview("Awaiting Pieces") {
    NavigationStack { ActiveGameView() }
        .environment(BoardConnection(
            gameState: GameState(status: .awaitingPieces, turn: .white)
        ))
}
#Preview("In Progress") {
    NavigationStack { ActiveGameView() }
        .environment(BoardConnection(
            gameState: GameState(status: .inProgress, turn: .white)
        ))
}
#Preview("Checkmate") {
    NavigationStack { ActiveGameView() }
        .environment(BoardConnection(
            gameState: GameState(status: .checkmate, turn: .black)
        ))
}
#Preview("Resignation") {
    NavigationStack { ActiveGameView() }
        .environment(BoardConnection(
            gameState: GameState(status: .resignation, turn: .white)
        ))
}
#Preview("Stalemate") {
    NavigationStack { ActiveGameView() }
        .environment(BoardConnection(
            gameState: GameState(status: .stalemate, turn: .white)
        ))
}
#endif
```

**ContentView:**

```swift
#if DEBUG
#Preview("Ready - New Game") {
    ContentView()
        .environment(BoardConnection())
}
#Preview("Scanning") {
    ContentView()
        .environment(BoardConnection(connectionState: .scanning))
}
#Preview("In Progress") {
    ContentView()
        .environment(BoardConnection(
            gameState: GameState(status: .inProgress, turn: .white)
        ))
}
#endif
```

## Test Changes

### Deleted tests

`WifiManagerTests.swift` and `LichessManagerTests.swift` are deleted. Their coverage is partially replaced by `BoardConnectionTests` and partially by the existing model decode tests. The old manager tests covered: (a) decoding notifications and updating status, (b) handling write errors, and (c) reset. Item (a) was largely redundant with `WifiStatusTests`/`LichessStatusTests` which test the decode logic directly. Items (b) and (c) tested trivial assignments. The BLE notification *routing* logic (matching characteristic UUIDs to the correct status property) moves into `BLETransport` and is not unit-tested -- it requires a real `CBPeripheral` to exercise. This is an acceptable tradeoff: the routing is a simple switch statement, and integration testing on a physical device covers it.

### New tests

`BoardConnectionTests.swift` tests `BoardConnection`'s command methods and state routing using the debug init (no transport, no BLE):

```swift
func testConfigureWifiSetsConnecting() {
    let board = BoardConnection(connectionState: .ready)
    board.configureWifi(ssid: "test", password: "pass", authMode: .wpa2)
    XCTAssertEqual(board.wifiStatus.state, .connecting)
}

func testSetLichessTokenSetsValidating() {
    let board = BoardConnection(connectionState: .ready)
    board.setLichessToken("lip_abc123")
    XCTAssertEqual(board.lichessStatus.state, .validating)
}

func testConfigureWifiNoOpWithoutTransport() {
    let board = BoardConnection(connectionState: .ready)
    board.configureWifi(ssid: "test", password: "pass", authMode: .wpa2)
    // Status changes to .connecting (local state update happens)
    // but no BLE write occurs (transport is nil)
    XCTAssertEqual(board.wifiStatus.state, .connecting)
}
```

The existing model decode tests (`WifiStatusTests`, `LichessStatusTests`, `GameStateTests`, `CommandResultTests`, `PlayerTypeTests`, `WifiConfigTests`) are unchanged.

## Files Modified / Created / Deleted

| Action | Path | Description |
|--------|------|-------------|
| Create | `Sources/BLE/BoardTransport.swift` | `BoardTransport` protocol definition |
| Create | `Sources/BLE/BLETransport.swift` | CoreBluetooth implementation extracted from `BLEDelegate` |
| Modify | `Sources/BLE/BoardConnection.swift` | Remove `BLEDelegate`, add transport property, fold WiFi/Lichess state and commands, add `#if DEBUG` init |
| Delete | `Sources/BLE/WifiManager.swift` | State and commands folded into `BoardConnection` |
| Delete | `Sources/BLE/LichessManager.swift` | State and commands folded into `BoardConnection` |
| Modify | `Sources/App/ChessBoardApp.swift` | Add `#if DEBUG && targetEnvironment(simulator)` gate |
| Modify | `Sources/Views/NewGameView.swift` | Replace `wifiManager`/`lichessManager` references, add `#if DEBUG` init for preview player types, add `#Preview` macros |
| Modify | `Sources/Views/ScanView.swift` | Add `#Preview` macros |
| Modify | `Sources/Views/ActiveGameView.swift` | Add `#Preview` macros |
| Modify | `Sources/Views/ContentView.swift` | Add `#Preview` macros |
| Create | `Tests/BoardConnectionTests.swift` | Tests for command methods and state transitions |
| Delete | `Tests/WifiManagerTests.swift` | Replaced by `BoardConnectionTests` |
| Delete | `Tests/LichessManagerTests.swift` | Replaced by `BoardConnectionTests` |
| Modify | `project.yml` | No structural change needed (no Preview Content directory) |

Unchanged: `GATT.swift`, all model files, `KeychainStore.swift`, all model test files.
