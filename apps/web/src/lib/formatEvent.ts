import type { GameEvent, PlayerAction } from "../types/api";
import { RANK_LABEL, SUIT_SYMBOL } from "./cardDisplay";

function formatAction(action: PlayerAction): string {
  if (action === "Fold") return "folded";
  if (action === "Check") return "checked";
  if (action === "Call") return "called";
  if (action === "AllIn") return "went all-in";
  if ("Bet" in action) return `bet ${action.Bet.amount}`;
  if ("Raise" in action) return `raised to ${action.Raise.amount}`;
  return String(action);
}

export function formatEvent(event: GameEvent): string {
  if (event === "HandFinished") return "Hand finished";
  if ("HandStarted" in event) {
    return `Hand started — dealer seat ${event.HandStarted.dealer_seat}`;
  }
  if ("HoleCardsDealt" in event) {
    return `Hole cards dealt to seats ${event.HoleCardsDealt.seats.join(", ")}`;
  }
  if ("BlindsPosted" in event) {
    const b = event.BlindsPosted;
    return `Blinds: seat ${b.small_blind_seat} posts ${b.small_blind}, seat ${b.big_blind_seat} posts ${b.big_blind}`;
  }
  if ("BoardRevealed" in event) {
    const cards = event.BoardRevealed.cards
      .map((c) => `${RANK_LABEL[c.rank]}${SUIT_SYMBOL[c.suit]}`)
      .join(" ");
    return `${event.BoardRevealed.phase}: ${cards}`;
  }
  if ("PhaseAdvanced" in event) {
    return `Phase → ${event.PhaseAdvanced.phase}`;
  }
  if ("PlayerActed" in event) {
    return `Seat ${event.PlayerActed.seat} ${formatAction(event.PlayerActed.action)}`;
  }
  if ("PotAwarded" in event) {
    const payouts = event.PotAwarded.payouts
      .map((p) => `seat ${p.seat} wins ${p.amount}`)
      .join(", ");
    return `🏆 Pot ${event.PotAwarded.total_pot} — ${payouts}`;
  }
  return JSON.stringify(event);
}
