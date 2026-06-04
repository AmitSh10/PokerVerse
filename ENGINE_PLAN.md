# PokerVerse Game Engine Plan

## Purpose

The game engine is the authoritative Rust domain layer for Texas Hold'em rules.

It should be deterministic, testable, and independent from the web server, database, WebSockets, and React frontend. The backend API and real-time layer will call into the engine, but they should not own game rules.

## Core Principle

All game truth lives in Rust:

- Turn order
- Legal actions
- Betting validation
- Pot calculation
- Card dealing
- Phase transitions
- Winner evaluation
- Chip movement

The frontend only renders state and sends player intentions.

## Suggested Crate Shape

Start with a Rust workspace containing a dedicated engine crate:

```text
pokerverse/
  Cargo.toml
  crates/
    poker_engine/
      Cargo.toml
      src/
        lib.rs
        action.rs
        betting.rs
        card.rs
        deck.rs
        engine.rs
        error.rs
        event.rs
        hand_eval.rs
        player.rs
        pot.rs
        state.rs
        table.rs
```

The engine crate should expose domain types and pure operations. Axum, SQLx, WebSockets, and Tokio should live outside this crate.

## Main Engine Flow

The central API should eventually feel like this:

```rust
let events = engine.apply_action(player_id, PlayerAction::Call)?;
let snapshot = engine.snapshot_for(player_id);
```

Actions mutate internal state only after validation. Every successful mutation emits events.

```text
PlayerAction -> validate -> mutate GameState -> emit GameEvents
```

## Core Domain Types

### Cards

```rust
enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

enum Rank {
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
    Ace,
}

struct Card {
    rank: Rank,
    suit: Suit,
}
```

### Game Phases

```rust
enum GamePhase {
    WaitingForPlayers,
    StartingHand,
    PostingBlinds,
    PreFlop,
    Flop,
    Turn,
    River,
    Showdown,
    HandComplete,
}
```

### Player Actions

```rust
enum PlayerAction {
    Fold,
    Check,
    Call,
    Bet { amount: u64 },
    Raise { amount: u64 },
    AllIn,
    SitOut,
    LeaveTable,
}
```

### Events

```rust
enum GameEvent {
    PlayerSat,
    HandStarted,
    BlindsPosted,
    CardsDealt,
    PlayerActed,
    PotUpdated,
    PhaseAdvanced,
    BoardRevealed,
    PlayerWonPot,
    HandCompleted,
}
```

Events are important because they can later power:

- WebSocket broadcasts
- Frontend animations
- Hand history
- Debug logs
- Replay support

## State Ownership

The engine should use normal owned Rust structs internally.

Avoid placing `Arc`, `Mutex`, or `RwLock` inside the core engine unless there is a strong reason. Concurrency belongs in the room/server layer:

```rust
RoomManager
  -> HashMap<RoomId, Arc<RwLock<GameEngine>>>
```

This keeps the domain model easier to reason about while still giving us Rust concurrency practice at the multiplayer boundary.

## Initial Module Responsibilities

### `card.rs`

Owns `Suit`, `Rank`, and `Card`.

Definition of done:

- All 52 card identities can be represented.
- Cards are comparable and printable for debugging.

### `deck.rs`

Owns deck creation, shuffling, and dealing.

Definition of done:

- A new deck contains 52 unique cards.
- Shuffling preserves all 52 unique cards.
- Dealing removes cards from the deck.
- Tests prove no duplicates are produced.

### `player.rs`

Owns player identity and chip state.

Definition of done:

- Player has an ID, display name, stack, seat, status, and hole cards.
- Player can be active, folded, all-in, sitting out, or disconnected.

### `table.rs`

Owns seats, blinds, dealer button, and table configuration.

Definition of done:

- Players can sit and leave.
- Seat count is enforced.
- Dealer button can advance.
- Small blind and big blind positions can be determined.

### `state.rs`

Owns game phase and hand-level state.

Definition of done:

- Current phase is explicit.
- Board cards are tracked.
- Acting player is tracked.
- Round contribution and total contribution are tracked.

### `betting.rs`

Owns legal action validation and betting transitions.

Definition of done:

- Check, call, bet, raise, fold, and all-in are validated.
- Current bet is tracked.
- Minimum raise rules are enforced.
- Turn advances correctly after action.

### `pot.rs`

Owns pot and side-pot calculation.

Definition of done:

- Main pot is calculated correctly.
- Folded players cannot win pots.
- Side pots are supported for all-in players.

### `hand_eval.rs`

Owns best-hand evaluation.

Definition of done:

- Best 5-card hand is selected from 7 available cards.
- All Texas Hold'em hand ranks are supported.
- Ties are supported.
- Split pots can be determined.

### `engine.rs`

Coordinates table state, hand state, betting, dealing, and events.

Definition of done:

- `GameEngine::new(config)` creates a table.
- `apply_action` validates and mutates state.
- `start_hand` starts a valid hand when enough players are seated.
- Engine emits domain events for successful state changes.

## Build Order

1. Cards and deck
2. Table seats and player stacks
3. Hand startup and blind posting
4. Hole card dealing
5. Betting round validation
6. Phase transitions
7. Board dealing
8. Pot calculation
9. Hand evaluation
10. Winner payout
11. Public/private snapshots
12. WebSocket integration

## Testing Strategy

The engine should be heavily tested before networking exists.

Priority tests:

- Deck has 52 unique cards.
- Dealing reduces deck size.
- Cannot start hand with fewer than 2 active players.
- Blinds are posted correctly.
- Acting player after blinds is correct.
- Player cannot act out of turn.
- Player cannot check when facing a bet.
- Player cannot bet more than their stack except via all-in semantics.
- Betting round ends only when all active players have matched or folded.
- Board reveals 3 cards on flop, 1 on turn, 1 on river.
- Best hand is evaluated correctly for every hand rank.
- Split pots work for ties.
- Side pots work for all-in players.

## Learning Targets By Phase

### Cards and Deck

Rust concepts:

- Enums
- Structs
- Trait derives
- Ownership of collections
- Unit tests

### Table and Players

Rust concepts:

- `HashMap`
- `Vec`
- `Option`
- Borrowing across collections
- Domain modeling

### Betting Engine

Rust concepts:

- Error handling with `Result`
- State transitions
- Pattern matching
- Invariant enforcement

### Pot and Hand Evaluation

Rust concepts:

- Sorting
- Iterators
- Custom ordering
- Exhaustive test cases

### Room Integration

Rust concepts:

- `Arc`
- `RwLock`
- Tokio tasks
- WebSocket broadcasting
- Command/event boundaries

## Design Rules

- Prefer explicit domain types over primitive strings.
- Keep game logic out of React.
- Keep SQLx out of the engine crate.
- Keep Axum handlers thin.
- Return errors instead of silently ignoring invalid actions.
- Emit events for successful mutations.
- Write tests before adding WebSocket behavior.
- Delay advanced 3D animations until the rules are stable.

## First Implementation Milestone

Create the Rust workspace and implement:

- `Suit`
- `Rank`
- `Card`
- `Deck`
- `Deck::new_shuffled`
- `Deck::deal_one`
- `Deck::deal_many`
- Tests for uniqueness and dealing behavior

This gives us a small, confidence-building foundation before the larger table state machine arrives.
