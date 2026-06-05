// Mirrors Rust serde JSON shapes exactly. Do not add poker logic here.

export type ChipAmount = number;
export type PlayerId = { "0": number };
export type SeatIndex = { "0": number };
export type RoomId = { "0": number };

export type Suit = "Clubs" | "Diamonds" | "Hearts" | "Spades";

export type Rank =
  | "Two"
  | "Three"
  | "Four"
  | "Five"
  | "Six"
  | "Seven"
  | "Eight"
  | "Nine"
  | "Ten"
  | "Jack"
  | "Queen"
  | "King"
  | "Ace";

export interface Card {
  rank: Rank;
  suit: Suit;
}

export type PlayerStatus =
  | "Active"
  | "Folded"
  | "AllIn"
  | "SittingOut"
  | "Disconnected";

export type GamePhase =
  | "WaitingForPlayers"
  | "StartingHand"
  | "PostingBlinds"
  | "PreFlop"
  | "Flop"
  | "Turn"
  | "River"
  | "Showdown"
  | "HandComplete";

export interface PlayerSnapshot {
  id: PlayerId;
  display_name: string;
  seat: SeatIndex;
  stack: ChipAmount;
  status: PlayerStatus;
  hole_card_count: number;
  visible_hole_cards: Card[] | null;
}

export interface ContributionSnapshot {
  seat: SeatIndex;
  total: ChipAmount;
  round: ChipAmount;
}

export interface HandSnapshot {
  phase: GamePhase;
  acting_seat: SeatIndex | null;
  board: Card[];
  pot: ChipAmount;
  current_bet: ChipAmount;
  dealer_seat: SeatIndex;
  small_blind_seat: SeatIndex;
  big_blind_seat: SeatIndex;
  first_to_act_seat: SeatIndex;
  contributions: ContributionSnapshot[];
}

export interface GameSnapshot {
  players: PlayerSnapshot[];
  hand: HandSnapshot | null;
}

export interface TableConfig {
  max_seats: number;
  small_blind: ChipAmount;
  big_blind: ChipAmount;
  min_buy_in: ChipAmount;
  max_buy_in: ChipAmount;
}

export interface RoomSummary {
  id: RoomId;
  table_config: TableConfig;
  seat_count: number;
  seated_player_count: number;
  can_start_hand: boolean;
  active_phase: GamePhase | null;
}

// ── Events ────────────────────────────────────────────────────────────────────

export interface PayoutEvent {
  seat: SeatIndex;
  amount: ChipAmount;
}

export type GameEvent =
  | { HandStarted: { dealer_seat: SeatIndex; playing_seats: SeatIndex[] } }
  | { HoleCardsDealt: { seats: SeatIndex[]; cards_per_player: number } }
  | {
      BlindsPosted: {
        small_blind_seat: SeatIndex;
        small_blind: ChipAmount;
        big_blind_seat: SeatIndex;
        big_blind: ChipAmount;
      };
    }
  | { BoardRevealed: { phase: GamePhase; cards: Card[] } }
  | { PhaseAdvanced: { phase: GamePhase } }
  | { PlayerActed: { seat: SeatIndex; action: PlayerAction } }
  | { PotAwarded: { total_pot: ChipAmount; payouts: PayoutEvent[] } }
  | "HandFinished";

// ── Player Actions ────────────────────────────────────────────────────────────

export type PlayerAction =
  | "Fold"
  | "Check"
  | "Call"
  | "AllIn"
  | { Bet: { amount: ChipAmount } }
  | { Raise: { amount: ChipAmount } };

// ── HTTP Response bodies ──────────────────────────────────────────────────────

export interface HealthResponse {
  status: "ok";
}

export interface ListRoomsResponse {
  rooms: RoomSummary[];
}

export interface CreateRoomRequest {
  id: RoomId;
  table_config: TableConfig;
}

export interface CreateRoomResponse {
  id: RoomId;
}

export interface RoomDetailsResponse {
  summary: RoomSummary;
  snapshot: GameSnapshot;
}

export interface RoomCommandResult {
  events: GameEvent[];
  snapshot: GameSnapshot;
}

export interface ApiErrorBody {
  code: string;
  error: string;
}

// ── WebSocket messages ────────────────────────────────────────────────────────

export type WsServerMessage =
  | { CommandResult: RoomCommandResult }
  | { Error: ApiErrorBody };
