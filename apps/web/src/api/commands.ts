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

export const commands = {
  sitPlayer(id: PlayerId, displayName: string, seat: SeatIndex, buyIn: ChipAmount): RoomCommand {
    return { SitPlayer: { id, display_name: displayName, seat, buy_in: buyIn } };
  },

  leaveSeat(seat: SeatIndex): RoomCommand {
    return { LeaveSeat: { seat } };
  },

  sitOut(seat: SeatIndex): RoomCommand {
    return { SitOut: { seat } };
  },

  sitIn(seat: SeatIndex): RoomCommand {
    return { SitIn: { seat } };
  },

  startHand(dealerSeat: SeatIndex): RoomCommand {
    return { StartHand: { dealer_seat: dealerSeat } };
  },

  advanceHandPhase(): RoomCommand {
    return "AdvanceHandPhase";
  },

  postBlinds(): RoomCommand {
    return "PostBlinds";
  },

  fold(seat: SeatIndex): RoomCommand {
    return { ApplyPlayerAction: { seat, action: "Fold" } };
  },

  check(seat: SeatIndex): RoomCommand {
    return { ApplyPlayerAction: { seat, action: "Check" } };
  },

  call(seat: SeatIndex): RoomCommand {
    return { ApplyPlayerAction: { seat, action: "Call" } };
  },

  bet(seat: SeatIndex, amount: ChipAmount): RoomCommand {
    return { ApplyPlayerAction: { seat, action: { Bet: { amount } } } };
  },

  raise(seat: SeatIndex, amount: ChipAmount): RoomCommand {
    return { ApplyPlayerAction: { seat, action: { Raise: { amount } } } };
  },

  allIn(seat: SeatIndex): RoomCommand {
    return { ApplyPlayerAction: { seat, action: "AllIn" } };
  },

  publicSnapshot(): RoomCommand {
    return "PublicSnapshot";
  },

  privateSnapshot(seat: SeatIndex): RoomCommand {
    return { PrivateSnapshot: { seat } };
  },
};
