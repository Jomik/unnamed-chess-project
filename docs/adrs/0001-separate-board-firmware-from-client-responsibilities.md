# Separate board firmware responsibilities from client responsibilities

## Context

This project is a smart chess board built around an ESP32-S3 microcontroller with Hall-effect sensors for piece detection and LEDs for move feedback. It communicates with external clients (currently an iOS app) over BLE, and supports game modes including human-vs-human, human-vs-engine, and human-vs-online-opponent (e.g., Lichess).

The current prototype treats the board firmware as a thick client: it runs chess logic, an embedded heuristic engine, direct HTTPS communication with Lichess, WiFi management, and game session orchestration -- all on the microcontroller. The iOS companion app is a thin BLE remote control that sends configuration and displays status. This was appropriate for proving out the concept but creates problems as the project grows:

- **Networking on the microcontroller is fragile and hard to debug.** ESP32 HTTP/TLS requires hand-rolled JSON parsing, works around known ESP-IDF bugs, and any Lichess API change requires a firmware update to a physical product.
- **Every new online service requires firmware changes.** Adding Chess.com, a custom engine server, or a cloud coaching service would each mean new HTTP client code in firmware.
- **Additional client types (Android, web, on-board display module) face unclear boundaries.** What must each client reimplement? What can they rely on the board for?

The project is at a natural reset point. All current code is prototype-quality. Migration cost is not a concern -- the architecture should be chosen on its own merits.

## Decision Drivers

- **Feedback latency.** When a player lifts a piece, LED feedback must appear within a single sensor tick (~50ms). Any architecture that routes piece-lift events through an external client for feedback computation adds 100-250ms of BLE round-trip latency, which is perceptible and unacceptable.
- **Client diversity.** iOS, Android, web clients, an on-board display module, and direct-connect services (laptop engine, cloud coaching) are all target consumers. The board should not be coupled to any specific client.
- **Firmware stability.** Firmware updates to a physical product are costly and risky. Features that change frequently (online services, coaching providers, game modes) should not require firmware changes.
- **Service independence.** The board should not contain Lichess-specific, Chess.com-specific, or any other service-specific code. External service integration is a client concern.
- **Simultaneous multi-client support.** A phone might orchestrate a Lichess game while a laptop provides coaching hints simultaneously. The board should support multiple connected clients fulfilling different roles.
- **Online game resilience.** If a client disconnects during an online game, the board's chess state must not be lost. Recovery should be possible on reconnection.

## Decision

The board firmware is a chess-aware sensor and display peripheral that exposes a transport-agnostic API. Clients and services connect to this API to orchestrate games, deliver opponent moves, and provide coaching hints. The firmware never contains service-specific networking code (no Lichess client, no Chess.com client, no cloud coaching client).

The firmware owns four concerns that cannot be delegated without unacceptable latency:

1. **Sensor reading** -- scanning Hall-effect sensors to produce piece-position bitboards.
2. **Chess logic** -- maintaining the authoritative chess position using a chess library, computing legal moves, and validating moves detected from sensor input.
3. **Move detection** -- matching sensor state changes against legal moves to determine what the human played. This requires chess logic and sensor data in the same tick loop.
4. **LED feedback computation** -- deriving what to display (move guidance, recovery hints, check indicators, coaching overlays) from the current position and sensor state. This is latency-critical and must run on-board.

Everything else is a client responsibility. From the board's perspective, the external world consists of three roles that any client or service can fulfill:

- **Game controller** -- configures and starts games, sends resign/draw commands. Typically the phone app.
- **Remote player** -- delivers opponent moves (from any source: online service, engine, another human on a remote board). The board validates and applies them identically regardless of source.
- **Coaching source** -- delivers hint overlays (squares to highlight) that the board merges into its LED feedback. The board does not know or care whether hints come from a phone engine, a cloud service, or a laptop.

A single client may fulfill all three roles (the phone app running a Lichess game with cloud coaching), or different clients may fulfill different roles simultaneously (phone provides the opponent, laptop provides coaching). The API contract is the same regardless of which client fills which role, and regardless of transport (BLE, WiFi, wired UART/SPI for an on-board module).

The board exposes the current chess position and detected moves through its API so that clients have sufficient context to fulfill their roles. The board is the single source of truth for the game position -- clients never independently track position from sensor data.

WiFi remains available on the board as a transport, alongside BLE and wired connections for physically-attached modules. This allows services to connect directly (a laptop engine over WiFi) without requiring a phone as an intermediary. WiFi network credentials are provided by a client (typically the phone app over BLE) before WiFi-based services can connect. Whether the board joins an existing network (STA mode) or hosts its own (SoftAP) is an implementation detail to be decided in a subsequent design, but the architecture supports either approach.

All existing specification documents (BLE companion app design, symmetric player abstraction, and others in `docs/specs/`) were written for the prototype architecture and are superseded by this decision. They should not be treated as constraints on designs that follow from this ADR.

## Consequences

- The firmware becomes a stable platform that rarely needs updating. New online services, game modes, and coaching providers are purely client-side additions.
- Each client or service only implements the roles it needs. A Lichess bridge does not need coaching logic. A coaching service does not need game orchestration. A phone app might do both.
- The on-board display module is architecturally just another client that happens to be wired to the board. If it wants to support Lichess directly, it runs its own Lichess bridge and speaks the same board API -- the firmware is unaware of Lichess.
- An embedded engine is not part of the firmware. Engine computation is a client/service concern, delivered to the board as a remote player move. A phone app, an on-board compute module, or a cloud service can all serve this role. This means the board cannot play human-vs-engine games without a connected client providing the engine.
- Exposing an API over WiFi means the board accepts inbound connections on the local network. Authentication and pairing must be part of the API design to prevent unauthorized clients from sending moves or altering LED state.
- Multiple transports (BLE, WiFi, wired) must be supported, which adds complexity to the firmware's communication layer. The API must be transport-agnostic, with transport-specific adapters.
- The API contract (moves in/out, coaching hints in, game lifecycle commands) becomes the critical interface to design carefully. Protocol versioning and backward compatibility matter because the board firmware and clients will evolve at different rates.
- Online game resilience depends on client reconnection behavior, not the firmware. The board holds the authoritative position and can resume from any point, but it cannot independently continue an online game if all clients disconnect. This is an accepted trade-off -- the firmware's job is to never lose state, not to maintain external connections.
- BLE's single-connection limit means that simultaneous multi-client over BLE alone is not possible. WiFi or wired connections are needed for additional clients beyond the primary BLE connection.

## Considered Options

### Board-authoritative with embedded networking

The board owns chess logic, game orchestration, and all networking (Lichess HTTP, WiFi management). Clients are thin remote controls that send configuration and display status. This is the current prototype architecture.

- Good, because client disconnect during an online game does not lose the game -- the board maintains the HTTP stream independently.
- Good, because clients are trivially simple to implement (just BLE config writes and status reads).
- Good, because BLE protocol is minimal (2-byte status notifications, one-shot config writes).
- Bad, because every new online service (Chess.com, custom engine server, cloud coaching) requires firmware changes and a firmware update to a physical product.
- Bad, because HTTP/TLS on ESP32 is the highest-risk, hardest-to-debug code in the system, and it runs on the platform with the worst debugging tools.
- Bad, because the phone must act as an intermediary for any service that wants to connect, even when direct communication would be simpler (e.g., a laptop engine).
- Bad, because coaching hints from external sources fight the architecture -- the board computes all feedback internally and has no inbound path for external hints.

### Thin firmware with smart clients

The board is a dumb sensor and LED peripheral. It streams raw sensor data over BLE and accepts LED frame buffers. All chess logic, move detection, feedback computation, and networking live in clients.

- Good, because firmware is trivially simple (~200 lines) and almost never needs updating.
- Good, because clients have maximum flexibility -- any chess logic, any UI, any service integration.
- Good, because coaching hints are natural (client just paints the LEDs).
- Bad, because piece-lift LED feedback requires a BLE round-trip (100-250ms added latency), which is perceptible and degrades the physical interaction.
- Bad, because every client must reimplement ~2000 lines of non-trivial chess logic (move detection from sensor deltas, feedback computation, bitboard manipulation).
- Bad, because the board is inert without a connected client -- no standalone capability for even basic play.
- Bad, because BLE bandwidth is stressed by 20Hz bidirectional sensor and LED streaming.

### Chess-aware board with transport-agnostic client API (chosen)

The board owns chess logic, move detection, and feedback computation for latency reasons. Everything else (game orchestration, online services, engine computation, coaching) is a client concern. The board exposes an API that any client or service can connect to over any transport.

- Good, because feedback latency stays at ~50ms (single tick, no external round-trip).
- Good, because new services and game modes are client-only additions -- no firmware updates.
- Good, because the API is role-based (game controller, remote player, coaching source), allowing flexible multi-client topologies.
- Good, because the board is transport-agnostic -- BLE, WiFi, and wired connections are all valid.
- Good, because the board never contains service-specific code (no Lichess, no Chess.com).
- Neutral, because the API contract is a new design surface that must be carefully specified and versioned.
- Bad, because online game resilience depends on client reconnection rather than being inherent to the board.
- Bad, because the board must support multiple transport layers (BLE, WiFi, wired), adding firmware complexity compared to BLE-only.
- Bad, because clients that want to display the game (board diagram, move list) must consume position data from the board API, adding protocol bandwidth.
- Bad, because the board cannot play against an engine without a connected client -- there is no standalone human-vs-computer mode.
