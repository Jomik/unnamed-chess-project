# BLE Companion App Design

## Overview

An iOS companion app that communicates with the chess board over BLE. The app serves as the sole configuration and control interface — the board is stateless (except sensor calibration) and requires the app to start a game.

This replaces the SoftAP browser-based provisioning flow. The board advertises BLE on boot and waits for the app to connect and configure it.

## Goals

- Configure WiFi and Lichess credentials from the app (no NVS config persistence)
- Start and control games: choose player types, resign
- Support human-vs-human and human-vs-Lichess AI game modes
- Provide real-time board status (WiFi state, Lichess connection, game state)

## Non-Goals

- Game viewer / PGN export (future consideration)
- Live game broadcast / spectator mode
- Android support (BLE protocol is client-agnostic; Android app can be added later)

## Architecture

### High-Level Components

```
┌──────────────────┐         BLE          ┌──────────────────┐
│   iOS App         │◄──────────────────►│   ESP32-S3        │
│   (BLE Central)   │                     │   (BLE Peripheral)│
│                   │                     │                   │
│  - Config UI      │   GATT services     │  - Game loop      │
│  - Game control   │   (3 services)      │  - Sensor input   │
│  - Chess clock    │                     │  - LED output     │
│  - Credential     │                     │  - WiFi client    │
│    storage        │                     │  - Lichess client │
└──────────────────┘                     └──────────────────┘
```

**Key constraint:** The ESP32 firmware is the single source of truth for game state. The app is a remote control — it sends commands and receives state updates but never owns the chess position.

### Stateless Board Design

The board persists nothing to NVS except sensor calibration (hardware-specific, separate `cal` partition). All user configuration — WiFi credentials, Lichess token, game setup — is sent by the app on each session.

The app stores credentials locally (iOS Keychain for sensitive values, UserDefaults for preferences).

**Boot sequence:**
1. Board powers on, initializes sensors and LEDs
2. Loads sensor calibration from NVS `cal` partition (falls back to defaults if uncalibrated)
3. Starts BLE advertising
4. Waits for app to connect and configure a game

**Rationale:** Since the app is required to start a game, there is no benefit to persisting config on the board. This eliminates the NVS config partition, the provisioning module, and the SoftAP code path entirely.

**Migration from SoftAP:** Existing boards with NVS config from the SoftAP provisioning flow will have stale data in the `config` NVS namespace. The new firmware ignores the `config` namespace entirely — it does not call `BoardConfig::load` and does not enter the SoftAP provisioning loop. The stale NVS data is harmless and can be cleaned up with `just erase-nvs` if desired.

## BLE Protocol

The board accepts one BLE connection at a time. No encryption or pairing is required.

### GATT Services

Three services, each grouping related concerns. All services use vendor-specific 128-bit UUIDs (generated once and fixed for the lifetime of the project). UUIDs are assigned in a registry section at the end of this document.

#### Binary Encoding Conventions

All characteristic payloads use the following conventions:

- **Integers**: Little-endian byte order
- **Strings**: Length-prefixed — 1 byte length (max 255), followed by UTF-8 bytes
- **Enums**: Single u8 byte with defined values per characteristic
- **Bitfields**: Single u8 byte, bits numbered from LSB

These conventions apply to all characteristics across all services.

#### Game Service

Game lifecycle and state. The core of the protocol (UUID prefix 1xxx).

| Characteristic    | Properties   | Payload                                                      |
| ------------------ | ------------ | ------------------------------------------------------------ |
| White Player       | Read, Write  | Tagged binary: type byte + type-specific config (see below)  |
| Black Player       | Read, Write  | Same format as White Player                                  |
| Start Game         | Write        | Empty (trigger)                                              |
| Match Control      | Write        | Action-based variable-length payload (see Match Control action format below) |
| Game State         | Read, Notify | Status (u8), turn (u8: 0x00=white, 0x01=black). Both fields are always present regardless of game status. The `turn` field reflects the side to move — it is not reinterpreted based on game outcome. In checkmate/stalemate this is the checkmated/stalemated side; in resignation/draw it is whoever's turn it was when the game ended. |
| Command Result     | Read, Notify | Status code (u8: 0x00=ok, 0x01=error), command source (u8: 0x00=StartGame, 0x01=MatchControl), error message (length-prefixed string, empty if ok) |

**Match Control action values and payload format:**

| Value | Action          | Payload Format       |
| ----- | --------------- | -------------------- |
| 0x00  | Resign          | `[0x00, color: u8]` where color is `0x00`=white, `0x01`=black |
| 0x01  | Cancel Game     | `[0x01]` (no additional bytes) |

**Command source values:**

| Value | Source       |
| ----- | ------------ |
| 0x00  | Start Game   |
| 0x01  | Match Control |

**Reconnection behavior:** On BLE reconnect, the app reads Game State to recover current state. The firmware resets Command Result to `[0x00, 0x00, 0x00]` (ok, StartGame, zero-length message) on reconnect — stale results from before disconnection are discarded.

**Player config initial state:** On boot (and after a game ends), White Player and Black Player read as `0xFF` (not a valid player type). The app must write both before Start Game is accepted.

**Command Result behavior:** Every write to Start Game or Match Control produces a Command Result notification — `[0x00, source, 0x00]` on success, `[0x01, source, msg_len, msg...]` on failure. The `source` byte echoes which command produced the result, allowing the app to correlate responses without tracking state. Error conditions include:
- Start Game written before both players are configured (either still reads `0xFF`)
- Start Game written while a game is already in progress
- Match Control (Resign) written when no game is in progress
- Match Control (CancelGame) written when no game is in progress → error "no game in progress"
- Match Control (CancelGame) in AwaitingPieces or InProgress state → success, resets board to Idle
- Lichess AI level out of range (must be 1–8)
- Unrecognized action or player type values

**Game State lifecycle:** After a game ends (checkmate, resignation, draw), Game State remains at the terminal status until the app writes Start Game again. A successful Start Game resets Game State to `0x01` (awaiting pieces) while the player sets up the board, then advances to `0x02` (in progress) once pieces are in their starting positions. The board does not automatically reset to `0x00` (idle) — the terminal state is preserved so the app can display the game result.

##### Player Config Tagged Format

The first byte determines the player type; remaining bytes are type-specific:

```
Human:           [0x00]
Embedded Engine: [0x01]
Lichess AI:      [0x02] [level: u8 (1–8)]
```

##### Game Status Values

| Value | Status          |
| ----- | --------------- |
| 0x00  | Idle            |
| 0x01  | Awaiting pieces |
| 0x02  | In progress     |
| 0x03  | Checkmate       |
| 0x04  | Stalemate       |
| 0x05  | Resignation     |
| 0x06  | Draw            |

##### Command Flow Examples

**Starting a human-vs-embedded game:**
```
App writes White Player  → [0x00]                       (human)
App writes Black Player  → [0x01]                       (embedded engine)
App writes Start Game    → (empty)
Board notifies Command Result → [0x00] [0x00] [0x00]    (ok, StartGame, no message)
Board notifies Game State     → [0x01] [0x00]           (awaiting pieces, white's turn)
  ... pieces placed on starting squares ...
Board notifies Game State     → [0x02] [0x00]           (in progress, white's turn)
```

**Starting a Lichess AI game:**
```
App writes WiFi Config   → [0x01] [SSID...] [password...]
App watches WiFi Status  → ... → connected
App writes Lichess Token → [token...]
App watches Lichess Status → ... → connected
App writes White Player  → [0x00]                       (human)
App writes Black Player  → [0x02] [0x04]               (Lichess AI, level 4)
App writes Start Game    → (empty)
```

**Resigning a game:**
```
App writes Match Control → [0x00] [0x00]                 (resign, white)
Board notifies Command Result → [0x00] [0x01] [0x00]     (ok, MatchControl, no message)
Board notifies Game State → [0x05] ...                  (resignation)
```

**Cancelling a game:**
```
App writes Match Control → [0x01]                        (cancel game)
Board notifies Command Result → [0x00] [0x01] [0x00]     (ok, MatchControl, no message)
Board notifies Game State → [0x00] ...                  (idle)
```


#### WiFi Service

Manages WiFi connectivity (UUID prefix 2xxx). The app writes credentials; the board attempts connection and reports status.

| Characteristic   | Properties   | Payload                                                       |
| ----------------- | ------------ | ------------------------------------------------------------- |
| WiFi Config       | Write        | Auth mode (u8), SSID (length-prefixed string), password (length-prefixed string) |
| WiFi Status       | Read, Notify | State (u8), error message (length-prefixed string)            |

**Auth mode values:**

| Value | Mode    |
| ----- | ------- |
| 0x00  | Open    |
| 0x01  | WPA2    |
| 0x02  | WPA3    |

**WiFi state values:**

| Value | State        |
| ----- | ------------ |
| 0x00  | Disconnected |
| 0x01  | Connecting   |
| 0x02  | Connected    |
| 0x03  | Failed       |

Unrecognized auth mode values cause WiFi Status to transition to `0x03` (Failed) with an error message describing the problem.

**Flow:** App writes WiFi Config → subscribes to WiFi Status → watches state transition from disconnected → connecting → connected (or failed with error message).

#### Lichess Service

Manages Lichess API connectivity (UUID prefix 3xxx). Requires WiFi to be connected first.

| Characteristic   | Properties   | Payload                                                       |
| ----------------- | ------------ | ------------------------------------------------------------- |
| Lichess Token     | Write        | API token (length-prefixed string)                             |
| Lichess Status    | Read, Notify | State (u8), error message (length-prefixed string)             |

**Lichess state values:**

| Value | State      |
| ----- | ---------- |
| 0x00  | Idle       |
| 0x01  | Validating |
| 0x02  | Connected  |
| 0x03  | Failed     |

**Flow:** App writes Lichess Token → board validates token against the Lichess API → Lichess Status updates.

## Firmware Integration

### BLE Stack

The ESP32-S3 uses NimBLE via the `esp32-nimble` crate (compatible with `esp-idf-svc` 0.52.1). NimBLE is the preferred BLE-only stack — lighter footprint than Bluedroid.

### Threading Model

BLE runs on NimBLE's host task (managed by ESP-IDF's FreeRTOS). Communication with the game loop uses an `mpsc` channel — the same pattern as the existing Lichess worker.

```
BLE thread                          Game loop thread
─────────                           ────────────────
GATT write received                 loop {
  → parse command                     sensors = read_positions()
  → send to channel ──────────────►   drain command channel
                                      session.tick(sensors)
Game State notify ◄───────────────    if state changed → update characteristic
                                      display.show(feedback)
                                      delay(50ms)
                                    }
```

- **Inbound commands** (player config, start game, match control): BLE callbacks push to an `mpsc` channel. The game loop drains it each tick.
- **Outbound state** (game state): After each tick, if state changed, the game loop writes to the Game State characteristic and triggers a notification.
- **WiFi/Lichess config writes**: Handled on the BLE thread directly (or spawned to a connection-management thread). These don't interact with the game loop.

### Coexistence with WiFi

ESP32-S3 supports BLE + WiFi simultaneously. BLE is always active. WiFi connects on demand when the app provides credentials for a Lichess game.


## iOS Companion App

### Tech Stack

SwiftUI + CoreBluetooth. No third-party dependencies.

### Screen Flow

```
Not connected → ScanView (scanning animation, auto-connects on discovery)
  ├─ Timeout (20s) → failure UI (Retry → restart scan)
  │   ├─ Scan timeout → "Board not found"
  │   ├─ Connect timeout → "Connection failed"
  │   └─ Setup timeout → "Setup failed"
  │
  └─ Connected, idle (0x00) → NewGameView
       ├─ White Player config (type picker + type-specific fields)
       ├─ Black Player config (same)
       ├─ If Lichess selected → WiFi setup (if not connected) → token entry
       └─ Start → writes characteristics, watches for Command Result errors
            │
            └─ Game active (0x01–0x06) → ActiveGameView
                 ├─ Awaiting pieces (0x01): prompt to place pieces on starting squares
                 ├─ In progress (0x02): turn indicator + match controls (resign)
                 └─ Terminal (0x03–0x06): game result display + "New Game" button
                      └─ "New Game" → navigates back to NewGameView
```

### State Management

The app is a thin client. All game state comes from BLE notifications. The app stores only:

- WiFi credentials → iOS Keychain
- Lichess API token → iOS Keychain
- User preferences (last-used config) → UserDefaults

A single `BoardConnection` class wraps CoreBluetooth and exposes observable state to SwiftUI views. No additional architecture (MVVM, Combine pipelines) unless complexity demands it later.

### Reconnection

If BLE disconnects, the app re-initiates connection from the `didDisconnectPeripheral` delegate callback by calling `centralManager.connect` again. On reconnect, the app reads current Game State, White Player, Black Player, WiFi Status, and Lichess Status. The firmware retains WiFi and Lichess status across BLE reconnections (only Command Result is reset on reconnect). Navigation is driven reactively by the observable `gameState` — once `connectionState` reaches `.ready`, `ContentView` displays `NewGameView` when status is idle (0x00), or `ActiveGameView` for any other status (0x01–0x06). No explicit navigation action is required on reconnect.

If WiFi drops during an active Lichess game, the board's Lichess connection breaks and the game ends via the board's game state transition (the firmware moves to a terminal status). The app surfaces this through the existing Game State notification path — no WiFi-specific UI is needed in ActiveGameView.

### Project Structure

```
companion/
  ChessBoard/
    project.yml           # XcodeGen spec (generates .xcodeproj)
    Sources/
      App/              # SwiftUI app entry point
      Views/            # Screens (scan, new game, active game)
      BLE/              # CoreBluetooth manager, GATT UUIDs, characteristic codecs
      Models/           # Player config, game state — mirrors firmware types
      Persistence/      # Keychain credential storage
    Tests/              # XCTest unit tests (protocol codecs)
```

## Repository Structure

The iOS app lives in the same repository as the firmware. CI uses path-filtered workflows.

```
unnamed-chess-project/
  src/                    # Rust firmware (unchanged)
  companion/              # iOS app
    ChessBoard/
      project.yml         # XcodeGen spec
      Sources/
      Tests/
  docs/specs/
  Cargo.toml
  justfile
  .github/workflows/
    rust_ci.yml           # triggers on **/*.rs, Cargo.*, .github/workflows/**
```

The BLE protocol (GATT service UUIDs, characteristic formats, byte encodings) is documented in this spec and serves as the contract between firmware and app.

## Phasing

### GATT Service Stability Across Phases

All three GATT services are registered from Phase 1 onward. In Phase 1, WiFi and Lichess service characteristics return `0x06 Request Not Supported` (ATT protocol) until implemented in Phase 2. This avoids CoreBluetooth service caching issues — bonded iOS devices cache discovered services, and adding new services between phases can cause discovery failures without a Bluetooth cache clear.

### Phase 1 — BLE Foundation + Start a Game

**Firmware:**
- BLE GATT server with all three services registered (Game Service functional; WiFi and Lichess services return "not supported")
- White/Black Player, Start Game, Match Control, Game State, Command Result characteristics
- Command channel between BLE thread and game loop

**App:**
- Scan and connect screen
- New game screen: human vs embedded engine only
- Active game screen showing game state
- No WiFi, no Lichess, no coaching

**Validates:** Full BLE stack end-to-end — GATT definitions, characteristic encoding/decoding, CoreBluetooth integration, command flow from app to game loop.

### Phase 2 — WiFi + Lichess Integration

**Firmware:**
- Implement WiFi Service (config + status characteristics, stubbed in Phase 1)
- Implement Lichess Service (token + status characteristics, stubbed in Phase 1)
- Lichess AI player type
- Remove SoftAP provisioning code

**App:**
- WiFi configuration flow with status feedback
- Lichess token entry with validation
- Lichess AI player type in game setup

Each phase is independently shippable — the board works at every stage. The Phase 2 app requires Phase 2 firmware — connecting to Phase 1 firmware will result in ATT errors for WiFi/Lichess characteristics.


## Appendix: GATT UUID Registry

All UUIDs are vendor-specific 128-bit values sharing a common random base. They are assigned once and never change. Both firmware and app must use these exact UUIDs.

| Entity                   | UUID                                   |
| ------------------------ | -------------------------------------- |
| **Game Service**         | `3d6343a2-1001-44ea-8fc2-3568d7216866` |
| White Player             | `3d6343a2-1002-44ea-8fc2-3568d7216866` |
| Black Player             | `3d6343a2-1003-44ea-8fc2-3568d7216866` |
| Start Game               | `3d6343a2-1004-44ea-8fc2-3568d7216866` |
| Match Control            | `3d6343a2-1005-44ea-8fc2-3568d7216866` |
| Game State               | `3d6343a2-1006-44ea-8fc2-3568d7216866` |
| Command Result           | `3d6343a2-1007-44ea-8fc2-3568d7216866` |
| **WiFi Service**         | `3d6343a2-2001-44ea-8fc2-3568d7216866` |
| WiFi Config              | `3d6343a2-2002-44ea-8fc2-3568d7216866` |
| WiFi Status              | `3d6343a2-2003-44ea-8fc2-3568d7216866` |
| **Lichess Service**      | `3d6343a2-3001-44ea-8fc2-3568d7216866` |
| Lichess Token            | `3d6343a2-3002-44ea-8fc2-3568d7216866` |
| Lichess Status           | `3d6343a2-3003-44ea-8fc2-3568d7216866` |

UUIDs share a randomly generated base (`3d6343a2-xxxx-44ea-8fc2-3568d7216866`) with a varying 16-bit segment (the second group) for readability. The base does not use the Bluetooth SIG reserved range.
