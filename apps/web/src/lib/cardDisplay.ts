import type { Rank, Suit } from "../types/api";

export const RANK_LABEL: Record<Rank, string> = {
  Two: "2", Three: "3", Four: "4", Five: "5",
  Six: "6", Seven: "7", Eight: "8", Nine: "9",
  Ten: "10", Jack: "J", Queen: "Q", King: "K", Ace: "A",
};

export const SUIT_SYMBOL: Record<Suit, string> = {
  Clubs: "♣", Diamonds: "♦", Hearts: "♥", Spades: "♠",
};

export function isRedSuit(suit: Suit): boolean {
  return suit === "Hearts" || suit === "Diamonds";
}
