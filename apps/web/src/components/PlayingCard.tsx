import type { Card } from "../types/api";
import { RANK_LABEL, SUIT_SYMBOL, isRedSuit } from "../lib/cardDisplay";

export function PlayingCard({ card }: { card: Card }) {
  const red = isRedSuit(card.suit);
  return (
    <div
      className={`w-10 h-14 bg-white rounded-sm border border-gray-300 flex flex-col justify-between p-1 select-none ${red ? "text-red-600" : "text-gray-900"}`}
    >
      <span className="text-xs font-bold leading-none">{RANK_LABEL[card.rank]}</span>
      <span className="text-base font-bold leading-none self-center">
        {SUIT_SYMBOL[card.suit]}
      </span>
    </div>
  );
}

export function FaceDownCard() {
  return (
    <div className="w-10 h-14 bg-blue-800 rounded-sm border border-blue-600 flex items-center justify-center select-none">
      <span className="text-blue-300 text-lg">🂠</span>
    </div>
  );
}
