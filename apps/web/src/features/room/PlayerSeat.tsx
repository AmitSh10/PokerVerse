import { useState } from "react";
import type { HandSnapshot, PlayerSnapshot, SeatIndex } from "../../types/api";
import { PlayingCard, FaceDownCard } from "../../components/PlayingCard";
import { commands, type RoomCommand } from "../../api/commands";

interface PlayerSeatProps {
  seatIndex: SeatIndex;
  player: PlayerSnapshot | undefined;
  hand: HandSnapshot | null;
  viewerSeat: SeatIndex | null;
  onViewerSeatChange: (seat: SeatIndex | null) => void;
  onCommand: (cmd: RoomCommand) => Promise<void>;
}

const STATUS_BADGE: Record<string, string> = {
  Active: "bg-green-700 text-green-100",
  Folded: "bg-gray-700 text-gray-400",
  AllIn: "bg-red-800 text-red-200",
  SittingOut: "bg-yellow-800 text-yellow-200",
  Disconnected: "bg-gray-800 text-gray-500",
};

export function PlayerSeat({
  seatIndex,
  player,
  hand,
  viewerSeat,
  onViewerSeatChange,
  onCommand,
}: PlayerSeatProps) {
  const [showSitForm, setShowSitForm] = useState(false);
  const [name, setName] = useState("");
  const [buyIn, setBuyIn] = useState("500");
  const [pending, setPending] = useState(false);
  const [sitError, setSitError] = useState<string | null>(null);

  const isActing = hand?.acting_seat === seatIndex;
  const isViewer = viewerSeat === seatIndex;
  const isDealer = hand?.dealer_seat === seatIndex;
  const isSB = hand?.small_blind_seat === seatIndex;
  const isBB = hand?.big_blind_seat === seatIndex;

  const handleSit = async () => {
    if (!name.trim() || pending) return;
    setSitError(null);
    setPending(true);
    try {
      await onCommand(commands.sitPlayer(Date.now(), name.trim(), seatIndex, parseInt(buyIn, 10)));
      // Only close the form and set viewer seat when the command succeeded
      onViewerSeatChange(seatIndex);
      setShowSitForm(false);
      setName("");
    } catch (e) {
      setSitError((e as Error).message);
    } finally {
      setPending(false);
    }
  };

  const handleLeave = async (e: React.MouseEvent) => {
    e.stopPropagation();
    await onCommand(commands.leaveSeat(seatIndex));
    onViewerSeatChange(null);
  };

  return (
    <div
      className={`
        flex flex-col items-center gap-1 pointer-events-auto
        ${isActing ? "drop-shadow-[0_0_8px_rgba(250,204,21,0.8)]" : ""}
      `}
    >
      {/* Position badges */}
      <div className="flex gap-1 h-4">
        {isDealer && (
          <span className="bg-white text-gray-900 text-xs font-bold px-1.5 rounded-full leading-4">D</span>
        )}
        {isSB && (
          <span className="bg-blue-600 text-white text-xs font-bold px-1.5 rounded-full leading-4">SB</span>
        )}
        {isBB && (
          <span className="bg-purple-600 text-white text-xs font-bold px-1.5 rounded-full leading-4">BB</span>
        )}
      </div>

      {/* Seat card */}
      <div
        className={`
          w-28 rounded-lg border p-2 text-center cursor-pointer transition-all
          ${isActing ? "border-yellow-400 bg-gray-700" : "border-gray-600 bg-gray-800"}
          ${isViewer ? "ring-1 ring-blue-400" : ""}
        `}
        onClick={() => player && !isViewer && onViewerSeatChange(seatIndex)}
      >
        {player ? (
          <>
            <div className="text-xs text-gray-400 mb-0.5">S{seatIndex}</div>
            <div className="font-semibold text-white text-sm truncate">{player.display_name}</div>
            <div className="text-yellow-400 text-xs">{player.stack} chips</div>
            <span
              className={`inline-block mt-1 text-xs px-1.5 py-0.5 rounded ${STATUS_BADGE[player.status] ?? ""}`}
            >
              {player.status}
            </span>

            {/* Hole cards */}
            {player.hole_card_count > 0 && (
              <div className="flex gap-1 justify-center mt-2">
                {player.visible_hole_cards
                  ? player.visible_hole_cards.map((c, i) => (
                      <PlayingCard key={i} card={c} />
                    ))
                  : Array.from({ length: player.hole_card_count }).map((_, i) => (
                      <FaceDownCard key={i} />
                    ))}
              </div>
            )}

            {/* Leave seat */}
            {isViewer && (
              <button
                onClick={handleLeave}
                className="mt-2 text-xs text-red-400 hover:text-red-300"
              >
                Leave
              </button>
            )}
          </>
        ) : (
          <>
            <div className="text-xs text-gray-500 mb-1">S{seatIndex}</div>
            <button
              onClick={(e) => {
                e.stopPropagation();
                setShowSitForm(true);
              }}
              className="text-xs text-green-400 hover:text-green-300"
            >
              Sit here
            </button>
          </>
        )}
      </div>

      {/* Inline sit form */}
      {showSitForm && (
        <div className="bg-gray-900 border border-gray-600 rounded-lg p-2 w-36 flex flex-col gap-1.5 z-10">
          <input
            autoFocus
            type="text"
            placeholder="Name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleSit()}
            className="bg-gray-700 text-white text-xs px-2 py-1 rounded border border-gray-600 w-full"
          />
          <input
            type="number"
            placeholder="Buy-in"
            value={buyIn}
            onChange={(e) => setBuyIn(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleSit()}
            className="bg-gray-700 text-white text-xs px-2 py-1 rounded border border-gray-600 w-full"
          />
          {sitError && (
            <p className="text-red-400 text-xs leading-tight">{sitError}</p>
          )}
          <div className="flex gap-1">
            <button
              onClick={handleSit}
              disabled={pending}
              className="flex-1 bg-green-700 hover:bg-green-600 disabled:opacity-50 text-white text-xs py-1 rounded"
            >
              {pending ? "…" : "Sit"}
            </button>
            <button
              onClick={() => { setShowSitForm(false); setSitError(null); }}
              className="flex-1 bg-gray-700 hover:bg-gray-600 text-white text-xs py-1 rounded"
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {/* Chip contribution */}
      {hand && (() => {
        const contrib = hand.contributions.find((c) => c.seat === seatIndex);
        return contrib && contrib.round > 0 ? (
          <span className="text-yellow-300 text-xs">{contrib.round}</span>
        ) : null;
      })()}
    </div>
  );
}
