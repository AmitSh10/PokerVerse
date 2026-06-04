# PokerVerse

## Product Requirements Document (PRD)

### Version

1.0

### Project Type

Full Stack Multiplayer Web Application

### Primary Goal

Build a multiplayer Texas Hold'em Poker platform with a modern 3D game table experience.

The project's primary purpose is educational:

* Learn Rust through a real-world project.
* Gain experience with ownership, borrowing, smart pointers, concurrency, state machines, traits, and networking.
* Build a professional backend architecture.
* Create a modern React frontend with a visually immersive poker experience.
* Implement real-time multiplayer communication.

The project should prioritize code quality, architecture, and Rust concepts over rapid feature delivery.

---

# Vision

Create a multiplayer poker platform where players can:

* Create rooms
* Join rooms
* Sit at poker tables
* Play Texas Hold'em
* Manage chips
* Participate in betting rounds
* View statistics
* Experience a modern animated 3D poker environment

The system should feel similar to online poker applications while remaining intentionally small enough to be built by a single developer.

---

# High-Level Architecture

## Frontend

Responsibilities:

* Lobby UI
* Room management
* 3D table rendering
* Player interactions
* Animations
* Game state visualization
* User settings

Technology:

* React
* TypeScript
* TailwindCSS
* React Query
* React Router
* Three.js
* React Three Fiber
* Zustand

---

## Backend

Responsibilities:

* Authentication
* Room management
* Game engine
* Hand evaluation
* Turn management
* Chip management
* Pot calculation
* State synchronization
* Real-time communication

Technology:

* Rust
* Axum
* Tokio
* WebSockets
* Serde
* SQLx
* PostgreSQL

---

## Database

Responsibilities:

* Users
* Statistics
* Game history
* Room history
* Authentication data

Technology:

* PostgreSQL

---

# Learning Goals

The project should intentionally expose the developer to:

## Rust Ownership

Examples:

* Room ownership
* Player ownership
* Game state ownership

---

## Smart Pointers

Learn:

* Box
* Rc
* Arc
* Mutex
* RwLock

---

## Enums

Examples:

* GameState
* PlayerAction
* CardSuit
* CardRank

---

## Traits

Examples:

* HandEvaluator
* Persistence
* Broadcaster

---

## Concurrency

Examples:

* Room execution
* WebSocket broadcasting
* Timers
* Event processing

---

## State Machines

Poker is naturally a state machine.

States:

* WaitingForPlayers
* SmallBlind
* BigBlind
* PreFlop
* Flop
* Turn
* River
* Showdown
* Finished

---

# Core Features

## Authentication

### Features

* Register
* Login
* Logout
* Guest mode

### Definition of Done

* Users can create accounts
* Users can log in
* Session persists

---

## Lobby

### Features

* Room list
* Create room
* Join room
* Player count
* Table limits

### Definition of Done

* Room creation works
* Room list updates
* Players can join

---

## Room System

### Features

* Multiple tables
* Maximum seats
* Buy-in amount
* Spectator mode

### Definition of Done

* Players can enter rooms
* Players can sit
* Players can leave

---

## Poker Engine

### Features

* Deck generation
* Card shuffling
* Card dealing
* Turn management

### Definition of Done

* Valid deck
* Fair shuffle
* Correct dealing

---

## Betting Engine

### Features

* Check
* Call
* Raise
* Fold
* All-In

### Definition of Done

* Valid bets
* Pot calculation
* Turn validation

---

## Hand Evaluation

### Features

* High Card
* Pair
* Two Pair
* Three of a Kind
* Straight
* Flush
* Full House
* Four of a Kind
* Straight Flush
* Royal Flush

### Definition of Done

* Correct winner selection
* Tie support

---

## Real-Time Communication

### Features

* Live actions
* Live chip updates
* Live card updates

### Definition of Done

* All players see updates instantly

---

# 3D Table Experience

## Goal

Create a realistic poker environment.

---

## Features

### Poker Table

* Green felt
* Wooden frame
* Real lighting

### Cards

* Animated dealing
* Card flips
* Hover effects

### Chips

* Chip stacks
* Pot visualization
* Animated movements

### Camera

* Table overview
* Seat focus
* Smooth transitions

---

## Technology

Use:

* Three.js
* React Three Fiber

Avoid custom WebGL development.

---

# Phase Roadmap

---

# Phase 1

## Foundation

Tasks:

* Project setup
* Rust backend
* React frontend
* PostgreSQL
* Authentication

Definition of Done:

* User can register and login

---

# Phase 2

## Lobby System

Tasks:

* Room creation
* Room listing
* Room joining

Definition of Done:

* Multiple players can enter rooms

---

# Phase 3

## Poker Engine

Tasks:

* Deck
* Cards
* Turns
* States

Definition of Done:

* Full game cycle works

---

# Phase 4

## Betting Engine

Tasks:

* Blinds
* Bets
* Pot

Definition of Done:

* Complete betting flow

---

# Phase 5

## Winner Evaluation

Tasks:

* Hand ranking
* Winner selection

Definition of Done:

* Correct winner every round

---

# Phase 6

## WebSockets

Tasks:

* Live updates
* Synchronization

Definition of Done:

* Multiplayer experience works

---

# Phase 7

## 3D Experience

Tasks:

* Table
* Cards
* Chips
* Animations

Definition of Done:

* Realistic poker table

---

# Phase 8

## Statistics

Tasks:

* Wins
* Losses
* Earnings
* Hand history

Definition of Done:

* Player profile dashboard

---

# Stretch Goals

* Tournaments
* AI opponents
* Chat
* Friend system
* Replay system
* Spectator mode
* Achievements
* Leaderboards

---

# DOs

* Keep business logic in Rust
* Use React only for presentation
* Use WebSockets for gameplay
* Design around state machines
* Write tests for poker logic
* Separate room logic from networking
* Build incrementally

---

# DON'Ts

* Do not place game logic in React
* Do not use polling for gameplay
* Do not build custom rendering engines
* Do not optimize prematurely
* Do not introduce microservices
* Do not start with tournaments
* Do not start with AI players
* Do not start with advanced animations
* Do not build mobile support initially

---

# Success Criteria

A user can:

1. Register
2. Create a room
3. Invite another player
4. Sit at a table
5. Play a complete Texas Hold'em hand
6. See real-time updates
7. Experience a polished 3D poker environment

while all game rules, state transitions, chip calculations, and winner evaluation are executed exclusively by the Rust backend.
