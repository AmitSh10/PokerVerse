# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
cargo build
cargo build --release

# Test all crates
cargo test

# Test a single crate
cargo test -p poker_engine
cargo test -p poker_server
cargo test -p poker_api

# Run a single test by name (substring match)
cargo test -p poker_engine test_name_here

# Run the API server (listens on 127.0.0.1:3000 by default)
cargo run -p poker_api

# Override listen address
$env:POKERVERSE_API_ADDR="0.0.0.0:8080"; cargo run -p poker_api
```

## Architecture

Three-crate Cargo workspace with a strict layering rule: upper crates depend on lower ones, never the reverse.

```
poker_api      (Axum HTTP + WebSocket server)
    └── poker_server  (room management, command dispatch)
            └── poker_engine  (pure Texas Hold'em domain logic)
```

### poker_engine — pure domain layer

No async, no networking, no `Arc`/`Mutex`. All state lives in owned structs. Every mutation emits a `GameEvent` into `engine.events: Vec<GameEvent>`.

**Central coordinator**: `GameEngine` owns a `Table` (seats + players), an `Option<HandState>` (the active hand), and an `Option<Deck>`. Methods like `start_hand`, `advance_phase`, `post_blinds`, `apply_action`, and `resolve_showdown` drive the game forward.

**Phase state machine** (in `state.rs`):
```
WaitingForPlayers → StartingHand → PostingBlinds → PreFlop
→ Flop → Turn → River → Showdown → HandComplete
```
`HandState` tracks the current `GamePhase`, community cards, betting amounts, and which seat acts next.

**Side pots** (`pot.rs`): `calculate_side_pots(contributions)` handles all-in scenarios. The result is a `Vec<SidePot>` where each pot has its own eligible player set.

**Hand evaluation** (`hand_eval.rs`): `evaluate_best_hand` selects the best 5-card combination from up to 7 cards and returns an `EvaluatedHand` with a `HandRank` suitable for comparison.

**Snapshots** (`snapshot.rs`): `GameSnapshot` serializes engine state for network transport. Private snapshots include hole cards; public snapshots omit them.

### poker_server — room runtime

`Room` wraps a `GameEngine` behind an `Arc<RwLock<Room>>` (aliased `SharedRoom`). `RoomManager` owns a `HashMap<RoomId, SharedRoom>`.

Commands arrive as `RoomCommand` variants (`SitPlayer`, `LeaveSeat`, `StartHand`, `ApplyPlayerAction`, …). `command.rs` validates and dispatches them, returning a `RoomCommandResult` containing emitted events and the updated snapshot.

### poker_api — HTTP/WebSocket layer

`app(state: ApiState)` returns an Axum `Router`. `ApiState` holds a `RoomManager` and a broadcast channel map (`HashMap<RoomId, broadcast::Sender<CommandResult>>`).

**REST endpoints** (all under `/rooms`):
- `GET /rooms` — list rooms
- `POST /rooms` — create room
- `GET /rooms/{id}` — public snapshot
- `DELETE /rooms/{id}` — close room
- `GET /rooms/{id}/seats/{seat}/snapshot` — private snapshot
- `POST /rooms/{id}/commands` — execute command

**WebSocket**: `GET /rooms/{id}/ws` — client sends JSON `RoomCommand`, server broadcasts `CommandResult` to all subscribers in the room.

CORS is permissive (ready for local frontend dev). All commands go through the same `RoomCommand` / `RoomCommandResult` types regardless of transport.

## Key design rules (from ENGINE_PLAN.md)

- All game truth lives in `poker_engine`. Frontend only renders snapshots — it never computes game state.
- Domain newtypes over primitives: `PlayerId`, `SeatIndex`, `ChipAmount`, `RoomId`.
- Tests are written against `poker_engine` directly; avoid mocking the engine in higher-layer tests.
- New game features belong in `poker_engine` first, then surface through commands and snapshots.
