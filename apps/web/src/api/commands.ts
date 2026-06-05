import type { ChipAmount, PlayerAction, PlayerId, SeatIndex } from "../types/api";

// Centralized command builders. Wire format matches Rust serde externally-tagged enums.
// Update only here if the backend wire format changes.

export type RoomCommand =
  | { SitPlayer: { id: PlayerId; display_name: string; seat: SeatIndex; buy_in: ChipAmount } }
  | { LeaveSeat: { seat: SeatIndex } }
  | { SitOut: { seat: SeatIndex } }
  | { SitIn: { seat: SeatIndex } }
  | { StartHand: { dealer_seat: SeatIndex } }
  | "AdvanceHandPhase"
  | "PostBlinds"
  | { ApplyPlayerAction: { seat: SeatIndex; action: PlayerAction } }
  | "PublicSnapshot"
  | { PrivateSnapshot: { seat: SeatIndex } };

function seat(index: number): SeatIndex {
  return { "0": index };
}

function playerId(id: number): PlayerId {
  return { "0": id };
}

export const commands = {
  sitPlayer(id: number, displayName: string, seatIndex: number, buyIn: ChipAmount): RoomCommand {
    return { SitPlayer: { id: playerId(id), display_name: displayName, seat: seat(seatIndex), buy_in: buyIn } };
  },

  leaveSeat(seatIndex: number): RoomCommand {
    return { LeaveSeat: { seat: seat(seatIndex) } };
  },

  sitOut(seatIndex: number): RoomCommand {
    return { SitOut: { seat: seat(seatIndex) } };
  },

  sitIn(seatIndex: number): RoomCommand {
    return { SitIn: { seat: seat(seatIndex) } };
  },

  startHand(dealerSeat: number): RoomCommand {
    return { StartHand: { dealer_seat: seat(dealerSeat) } };
  },

  advanceHandPhase(): RoomCommand {
    return "AdvanceHandPhase";
  },

  postBlinds(): RoomCommand {
    return "PostBlinds";
  },

  fold(seatIndex: number): RoomCommand {
    return { ApplyPlayerAction: { seat: seat(seatIndex), action: "Fold" } };
  },

  check(seatIndex: number): RoomCommand {
    return { ApplyPlayerAction: { seat: seat(seatIndex), action: "Check" } };
  },

  call(seatIndex: number): RoomCommand {
    return { ApplyPlayerAction: { seat: seat(seatIndex), action: "Call" } };
  },

  bet(seatIndex: number, amount: ChipAmount): RoomCommand {
    return { ApplyPlayerAction: { seat: seat(seatIndex), action: { Bet: { amount } } } };
  },

  raise(seatIndex: number, amount: ChipAmount): RoomCommand {
    return { ApplyPlayerAction: { seat: seat(seatIndex), action: { Raise: { amount } } } };
  },

  allIn(seatIndex: number): RoomCommand {
    return { ApplyPlayerAction: { seat: seat(seatIndex), action: "AllIn" } };
  },

  publicSnapshot(): RoomCommand {
    return "PublicSnapshot";
  },

  privateSnapshot(seatIndex: number): RoomCommand {
    return { PrivateSnapshot: { seat: seat(seatIndex) } };
  },
};
