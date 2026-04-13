# Companion App Preview & Simulator Support Design

## Overview

The companion app cannot render any view in `#Preview` macros or run meaningfully in the iOS Simulator because every view depends on `BoardConnection`, which unconditionally creates a `CBCentralManager` on init. This design extracts a `BoardTransport` protocol to separate BLE I/O from application state, consolidates scattered manager state into `BoardConnection`, and introduces a mock transport for previews and tests.

## Goals

- All views render in `#Preview` macros with representative states.
- The app launches in the Simulator with a mock board connection instead of blocking on BLE scanning.
- Views continue to use `@Environment(BoardConnection.self)` -- no view-layer protocol abstraction, no existential types.
- The production BLE code path is preserved exactly.
- Existing model tests continue to pass without modification.

## Non-Goals

- Fully functional BLE simulation (replaying recorded BLE traffic, simulating board responses).
- Unit testing of view logic.
- Abstracting keychain or user defaults for preview isolation.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                       Views                          │
│  @Environment(BoardConnection.self) private var board│
└────────────────────────┬────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────┐
│                  BoardConnection                     │
│  @Observable — single source of truth                │
│  Owns all state, delegates I/O to transport          │
│  Transport is non-optional (private let)             │
└────────────────────────┬────────────────────────────┘
                         │ BoardTransport protocol
┌────────────────────────▼────────────────────────────┐
│  BLETransport (production)                           │
│  CBCentralManager + CBPeripheral delegates           │
│  Calls back via weak owner: BoardConnection?         │
├─────────────────────────────────────────────────────┤
│  MockTransport (#if DEBUG)                           │
│  Records write calls + lifecycle calls               │
│  Used in previews (via MockBoard) and tests          │
└─────────────────────────────────────────────────────┘
```

### BoardTransport

Minimal protocol defining the operations that vary between real BLE and mock: writing data to a characteristic, restarting/stopping scans, and cancelling connections. Plus a weak `owner` back-reference for the transport to push state updates to `BoardConnection`.

Everything else -- state decoding, state management, GATT characteristic routing, notification handling -- lives in the transport implementation and flows back through `owner`.

### BLETransport

Production implementation extracted from the existing BLE delegate code. Conforms to `BoardTransport` and the CoreBluetooth delegate protocols. All characteristic references and notification routing live here. Write errors are routed back to `BoardConnection` state properties via `owner`.

### BoardConnection

Single `@Observable` source of truth for all app state. WiFi and Lichess status (previously in separate manager objects) are direct properties, flattening property access for views. All command methods encode data and call `transport.write(...)`.

Single initializer taking a `BoardTransport`. The transport is non-optional -- production callers pass `BLETransport`, previews and tests pass `MockTransport`. No nil transport path.

### MockTransport

`#if DEBUG`-gated class conforming to `BoardTransport`. Records call counts and arguments for each protocol method. Lives alongside production BLE code (not in test targets) because `#Preview` macros in source files need access.

### MockBoard

`#if DEBUG`-gated `PreviewModifier` that creates a `MockTransport`-backed `BoardConnection`, sets requested state properties, and injects it into the SwiftUI environment. Defaults mirror the common preview case. Views use it via preview traits.

### Simulator Support

The app entry point uses a compile-time gate (`#if DEBUG && targetEnvironment(simulator)`) to pass `MockTransport` instead of `BLETransport`. Debug builds on physical devices still use real BLE.

## Testing Strategy

Tests construct `MockTransport` directly (not via `MockBoard`) and pass it to `BoardConnection`. This allows both state-based assertions and write-verification (asserting on recorded call counts and arguments).

BLE notification routing in `BLETransport` is not unit-tested -- it requires a real `CBPeripheral`. This is acceptable: the routing is a simple switch and is covered by integration testing on a physical device.
