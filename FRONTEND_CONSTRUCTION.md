# Frontend Construction Plan

This document defines what the PokerVerse frontend should contain, what technologies it should use, and how it should talk to the current Rust backend.

Frontend implementation should not begin until we explicitly confirm that the backend foundation is ready and confirm the move to frontend twice.

## Product Goal

Build a multiplayer poker client that renders the server-authoritative game state, lets players join seats and take actions, and makes the table feel clear in early testing and polished in production.

The frontend should not own poker rules. The Rust backend remains the source of truth for legal state transitions, player actions, cards, pots, hand phases, and winners.

## Recommended Stack

### Core App

- React with TypeScript
- Vite for development and production builds
- TailwindCSS for styling
- React Router for routes
- TanStack Query for HTTP requests, mutations, cache updates, and invalidation
- Zustand for local client state such as selected room, viewer seat, socket status, UI preferences, and debug toggles
- Native WebSocket wrapper for room realtime updates

### 2D Testing Layer

Use a 2D interface first so we can test gameplay quickly without fighting 3D complexity.

- CSS/Tailwind table layout
- Simple card components
- Chip and pot labels
- Action buttons
- Event/debug log
- Visible connection state
- Mobile-friendly responsive layout

### 3D Production Layer

After the 2D client proves the game loop, add a production table experience.

- Three.js
- React Three Fiber
- Drei helpers
- 3D table, seats, cards, chips, and camera
- Smooth card dealing and board reveal animations
- The 3D scene should render the same backend snapshots as the 2D client

The 2D client should remain available as a debug/testing mode even after 3D exists.

## Suggested Project Location

When we start the frontend, prefer this structure:

```text
apps/
  web/
    src/
      api/
      components/
      features/
      lib/
      routes/
      stores/
      types/
      ws/
```

This keeps Rust crates in `crates/` and frontend apps in `apps/`.

## Main Screens

- Lobby: list rooms, create room, enter room
- Table: board cards, player seats, pot, active player indicator, action controls
- Seat controls: sit, leave, sit out, sit in
- Player panel: stack, status, hole cards when allowed
- Event log: useful for testing and replaying game flow mentally
- Connection status: HTTP/server health and WebSocket connected/reconnecting/error states
- Debug panel: current snapshot JSON, command shortcuts, selected viewer seat

## Frontend State Model

Use server snapshots as the canonical game state.

- TanStack Query should cache room lists, room details, and snapshots.
- Zustand should store UI/session state that does not belong on the server.
- WebSocket messages should update or invalidate cached room data.
- The frontend should build typed commands, send them to the backend, then render the returned snapshot/events.

Do not duplicate backend poker logic in React. The frontend may calculate presentation-only values, like card positions or whether a button should look disabled, but the backend must validate every real command.

## Backend Base URLs

Default local backend:

```text
HTTP: http://127.0.0.1:3000
WS:   ws://127.0.0.1:3000
```

The backend address is configurable through `POKERVERSE_API_ADDR`. The frontend should use environment variables such as:

```text
VITE_API_BASE_URL=http://127.0.0.1:3000
VITE_WS_BASE_URL=ws://127.0.0.1:3000
```

## HTTP Endpoints

### Health

```http
GET /health
```

Purpose: check whether the API server is running.

Response:

```json
{
  "status": "ok"
}
```

### List Rooms

```http
GET /rooms
```

Purpose: render the lobby.

Response:

```json
{
  "rooms": [
    {
      "id": 1,
      "player_count": 2,
      "occupied_seats": [0, 3],
      "hand_in_progress": true
    }
  ]
}
```

### Create Room

```http
POST /rooms
```

Purpose: create a table.

Example request:

```json
{
  "id": 1,
  "table_config": {
    "max_seats": 6,
    "small_blind": 5,
    "big_blind": 10,
    "min_buy_in": 100,
    "max_buy_in": 1000
  }
}
```

Example response:

```json
{
  "id": 1
}
```

### Get Room

```http
GET /rooms/{room_id}
```

Purpose: load a room summary and public snapshot.

Response includes:

- `summary`
- `snapshot`

### Delete Room

```http
DELETE /rooms/{room_id}
```

Purpose: remove a room.

Response: `204 No Content`

### Get Private Seat Snapshot

```http
GET /rooms/{room_id}/seats/{seat}/snapshot
```

Purpose: load a snapshot from one seat's perspective, including visible hole cards for that seat when available.

Important: this is development-friendly but not secure enough for public production by itself. Before real deployment, we need authentication/session ownership so players cannot request another seat's private cards.

### Send Room Command

```http
POST /rooms/{room_id}/commands
```

Purpose: perform a game action through HTTP.

Response includes:

- `events`
- `snapshot`

The WebSocket route should be preferred for live gameplay, but this endpoint is useful for tests, debug panels, and non-realtime commands.

## WebSocket Endpoint

```http
GET /rooms/{room_id}/ws
```

Purpose: send room commands and receive realtime command results/errors.

Client sends JSON room commands as text messages.

Server sends JSON messages:

```json
{
  "CommandResult": {
    "events": [],
    "snapshot": {}
  }
}
```

```json
{
  "Error": {
    "code": "invalid_command",
    "error": "Human-readable error"
  }
}
```

The frontend WebSocket client should handle reconnects, stale room state, and command errors gracefully.

## Room Commands

The current backend uses Rust serde's default externally tagged enum shape. Keep command creation centralized in `src/api/commands.ts` or similar so we can change wire format later without touching UI components.

### Sit Player

```json
{
  "SitPlayer": {
    "id": 1,
    "display_name": "Ada",
    "seat": 0,
    "buy_in": 1000
  }
}
```

### Leave Seat

```json
{
  "LeaveSeat": {
    "seat": 0
  }
}
```

### Sit Out

```json
{
  "SitOut": {
    "seat": 0
  }
}
```

### Sit In

```json
{
  "SitIn": {
    "seat": 0
  }
}
```

### Start Hand

```json
{
  "StartHand": {
    "dealer_seat": 0
  }
}
```

### Post Blinds

```json
"PostBlinds"
```

### Advance Hand Phase

```json
"AdvanceHandPhase"
```

### Apply Player Action

Fold:

```json
{
  "ApplyPlayerAction": {
    "seat": 0,
    "action": "Fold"
  }
}
```

Check:

```json
{
  "ApplyPlayerAction": {
    "seat": 0,
    "action": "Check"
  }
}
```

Call:

```json
{
  "ApplyPlayerAction": {
    "seat": 0,
    "action": "Call"
  }
}
```

Bet:

```json
{
  "ApplyPlayerAction": {
    "seat": 0,
    "action": {
      "Bet": {
        "amount": 20
      }
    }
  }
}
```

Raise:

```json
{
  "ApplyPlayerAction": {
    "seat": 0,
    "action": {
      "Raise": {
        "amount": 40
      }
    }
  }
}
```

All in:

```json
{
  "ApplyPlayerAction": {
    "seat": 0,
    "action": "AllIn"
  }
}
```

### Public Snapshot

```json
"PublicSnapshot"
```

### Private Snapshot

```json
{
  "PrivateSnapshot": {
    "seat": 0
  }
}
```

## Snapshot Data To Render

The frontend should expect snapshots with these major concepts:

- Players: seat, display name, stack, status, hole card count, visible hole cards when allowed
- Hand: phase, acting seat, dealer seat, blind seats, board, pot, current bet, contributions
- Cards: rank and suit
- Events: hand started, cards dealt, blinds posted, board revealed, player acted, pot awarded, hand finished

The exact TypeScript types should mirror the Rust serde JSON shape and live in one place, likely `src/types/api.ts`.

## Error Handling

API errors use a structured body:

```json
{
  "code": "room_not_found",
  "error": "Room 1 was not found"
}
```

The frontend should show friendly messages for:

- Room not found
- Room already exists
- Invalid command
- Room runtime unavailable
- Bad request
- WebSocket disconnected

## Testing Plan

- Unit test command builders so JSON sent by React matches backend expectations.
- Unit test API parsing helpers.
- Component test action buttons for disabled/loading/error states.
- Component test 2D table rendering from fixed snapshots.
- Add end-to-end tests later with Playwright once the basic UI exists.

## Build Phases

### Phase 1: API Client And Types

- Create typed HTTP client.
- Create WebSocket room client.
- Create command builder helpers.
- Add environment variables.

### Phase 2: 2D Debug Client

- Build lobby.
- Build room page.
- Render public/private snapshots.
- Add seat controls and action controls.
- Add event log and debug JSON view.

### Phase 3: Realtime Gameplay UX

- Prefer WebSocket commands during active play.
- Add reconnect behavior.
- Keep snapshot cache synchronized.
- Make acting player and legal action flow obvious.

### Phase 4: Polished 2D Table

- Improve table layout, cards, chips, animations, and mobile behavior.
- Keep this mode available for testing.

### Phase 5: 3D Production Table

- Add React Three Fiber table scene.
- Render the same snapshots as the 2D table.
- Add camera, card, chip, and board animations.
- Preserve 2D fallback/debug mode.

## Before Frontend Implementation

Before creating frontend source files, we should do this checkpoint:

1. Confirm backend foundation is ready.
2. Ask whether to move to frontend.
3. Ask again for final confirmation before creating frontend code.

