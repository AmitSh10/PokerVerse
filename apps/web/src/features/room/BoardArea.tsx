import type { HandSnapshot } from "../../types/api";
import { PlayingCard } from "../../components/PlayingCard";

export function BoardArea({ hand }: { hand: HandSnapshot | null }) {
  if (!hand) {
    return (
      <div className="flex flex-col items-center gap-1">
        <p className="text-gray-500 text-sm">Waiting for hand to start</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col items-center gap-2">
      {/* Phase label */}
      <span className="text-xs text-gray-400 uppercase tracking-widest">{hand.phase}</span>

      {/* Community cards */}
      <div className="flex gap-1.5">
        {hand.board.map((card, i) => (
          <PlayingCard key={i} card={card} />
        ))}
        {Array.from({ length: Math.max(0, 5 - hand.board.length) }).map((_, i) => (
          <div
            key={`empty-${i}`}
            className="w-10 h-14 rounded-sm border border-dashed border-gray-600 bg-green-900/30"
          />
        ))}
      </div>

      {/* Pot */}
      <div className="flex items-center gap-2">
        <span className="text-gray-400 text-xs">Pot</span>
        <span className="text-yellow-400 font-bold">{hand.pot}</span>
        {hand.current_bet > 0 && (
          <>
            <span className="text-gray-600">·</span>
            <span className="text-gray-400 text-xs">Bet</span>
            <span className="text-white text-xs">{hand.current_bet}</span>
          </>
        )}
      </div>
    </div>
  );
}
