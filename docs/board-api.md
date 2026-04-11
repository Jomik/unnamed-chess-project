# Board API

Logical API surface for the chess board firmware. Defines the state, operations, events, and types that all transports must expose.

See `docs/adrs/0001-separate-board-firmware-from-client-responsibilities.md` for architectural context.

## State

The board exposes the following readable state. Each piece of state is independently accessible.

```rust
GameStatus   : GameStatus                  // current game lifecycle state
WhitePlayer  : PlayerType                  // white side's player type
BlackPlayer  : PlayerType                  // black side's player type
Position     : FEN string (optional)       // current chess position, absent when Idle
LastMove     : Color + UCI move (optional) // most recent move, absent when no move played
```

`LastMove` enables reconnecting clients to sync the last state transition without maintaining full move history.

## Operations

Actions a client can invoke on the board. Each returns success or a typed error.

### Game Lifecycle

```rust
StartGame(white: PlayerType, black: PlayerType) -> GameAlreadyInProgress
```

Transitions to `AwaitingPieces`. When the board detects the starting position on sensors, transitions to `InProgress`. Emits `GameStateChanged`.

```rust
CancelGame() -> NoGameInProgress
```

Transitions to `Idle`. Emits `GameStateChanged`.

### In-Game Actions

```rust
SubmitMove(move: UCI) -> NoGameInProgress | NotYourTurn | IllegalMove
```

Delivers a remote player's move (e.g. `e2e4`, `e1g1` for castling, `e7e8q` for promotion). Board validates and applies the move. Emits `MovePlayed` and `GameStateChanged`.

```rust
Resign(color: Color) -> NoGameInProgress | CannotResignForRemotePlayer
```

Resigns on behalf of a human side. Transitions to `Resigned { color }`. Emits `GameStateChanged`.

## Events

State changes the board pushes to connected clients.

```rust
GameStateChanged(status: GameStatus)
```

Emitted when `GameStatus` changes.

```rust
MovePlayed(color: Color, move: UCI)
```

Emitted when a move is played, regardless of source (human on the board or remote via `SubmitMove`).

## Types

### GameStatus

The game lifecycle state.

```rust
enum GameStatus {
    Idle,                        // No game in progress
    AwaitingPieces,              // Waiting for starting position on sensors
    InProgress,
    Checkmate { loser: Color },
    Stalemate,
    Resigned { color: Color },
}
```

### Color

```rust
enum Color {
    White,
    Black,
}
```

### PlayerType

Determines how moves arrive for a given side.

```rust
enum PlayerType {
    Human,  // Detected from sensors on the physical board
    Remote, // Delivered via SubmitMove
}
```

### Error

```rust
enum Error {
    GameAlreadyInProgress,
    NoGameInProgress,
    NotYourTurn,
    IllegalMove,
    CannotResignForRemotePlayer, // Resign is only valid for human sides
}
```

## Behavior

### Reconnection

Clients read the board's current state on connect. Move history is not maintained by the board -- clients track it from `MovePlayed` events.

### Multi-Client

Multiple clients may connect over different transports. All receive all events. Operations are processed in arrival order. Conflicting operations (e.g., two `SubmitMove` calls for the same turn) are resolved by order: first valid one applied, subsequent rejected.
