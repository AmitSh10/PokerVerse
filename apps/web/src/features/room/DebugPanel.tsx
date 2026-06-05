import { useEffect, useState } from "react";
import type { GameSnapshot, SeatIndex } from "../../types/api";
import { commands, type RoomCommand } from "../../api/commands";

interface DebugPanelProps {
  snapshot: GameSnapshot;
  viewerSeat: SeatIndex | null;
  onViewerSeatChange: (seat: SeatIndex | null) => void;
  onCommand: (cmd: RoomCommand) => Promise<void>;
  onQuickStart: (dealerSeat: SeatIndex) => Promise<void>;
  onRefresh: () => void;
  lastError: string | null;
}

export function DebugPanel({
  snapshot,
  viewerSeat,
  onViewerSeatChange,
  onCommand,
  onQuickStart,
  onRefresh,
  lastError,
}: DebugPanelProps) {
  const [showJson, setShowJson] = useState(false);
  const [pending, setPending] = useState(false);

  // Seats that have active players (valid dealer candidates)
  const playerSeats = snapshot.players
    .filter((p) => p.status === "Active" || p.status === "AllIn")
    .map((p) => p.seat);

  const [dealerSeat, setDealerSeat] = useState<number>(playerSeats[0] ?? 0);

  // Keep dealer seat valid when snapshot changes
  useEffect(() => {
    if (playerSeats.length > 0 && !playerSeats.includes(dealerSeat)) {
      setDealerSeat(playerSeats[0]);
    }
  }, [JSON.stringify(playerSeats)]); // eslint-disable-line react-hooks/exhaustive-deps

  const run = async (fn: () => Promise<void>) => {
    setPending(true);
    try {
      await fn();
    } finally {
      setPending(false);
    }
  };

  const shortcut = (label: string, cmd: RoomCommand) => (
    <button
      onClick={() => run(() => onCommand(cmd))}
      disabled={pending}
      className="bg-gray-700 hover:bg-gray-600 disabled:opacity-40 text-gray-200 text-xs px-3 py-1.5 rounded transition-colors"
    >
      {label}
    </button>
  );

  return (
    <div className="bg-gray-950 border-t border-gray-800 p-3 space-y-2">
      <div className="flex flex-wrap gap-2 items-center">
        {/* Viewer seat selector */}
        <div className="flex items-center gap-1.5">
          <span className="text-xs text-gray-500">View as</span>
          <select
            value={viewerSeat ?? ""}
            onChange={(e) =>
              onViewerSeatChange(e.target.value === "" ? null : Number(e.target.value))
            }
            className="bg-gray-700 text-white text-xs px-2 py-1 rounded border border-gray-600"
          >
            <option value="">— spectate —</option>
            {snapshot.players.map((p) => (
              <option key={p.seat} value={p.seat}>
                Seat {p.seat} ({p.display_name})
              </option>
            ))}
          </select>
        </div>

        <span className="text-gray-700">|</span>

        {/* Dealer + quick-start */}
        <div className="flex items-center gap-1.5">
          <span className="text-xs text-gray-500">Dealer</span>
          <select
            value={dealerSeat}
            onChange={(e) => setDealerSeat(Number(e.target.value))}
            disabled={playerSeats.length === 0}
            className="bg-gray-700 text-white text-xs px-2 py-1 rounded border border-gray-600 w-36 disabled:opacity-40"
          >
            {playerSeats.length === 0 ? (
              <option>No players seated</option>
            ) : (
              playerSeats.map((s) => {
                const name = snapshot.players.find((p) => p.seat === s)?.display_name ?? "";
                return (
                  <option key={s} value={s}>
                    Seat {s} ({name})
                  </option>
                );
              })
            )}
          </select>
          <button
            onClick={() => run(() => onQuickStart(dealerSeat))}
            disabled={pending || playerSeats.length < 2}
            className="bg-green-800 hover:bg-green-700 disabled:opacity-40 text-green-100 text-xs px-3 py-1.5 rounded font-semibold transition-colors"
            title="StartHand → AdvancePhase → PostBlinds → AdvancePhase (PreFlop)"
          >
            {pending ? "…" : "▶ Start Hand"}
          </button>
        </div>

        <span className="text-gray-700">|</span>

        {/* Manual phase controls */}
        {shortcut("Advance Phase", commands.advanceHandPhase())}
        {shortcut("Post Blinds", commands.postBlinds())}

        <span className="text-gray-700">|</span>

        <button
          onClick={onRefresh}
          className="text-xs text-gray-500 hover:text-gray-300 transition-colors"
        >
          ↻ Refresh
        </button>

        <button
          onClick={() => setShowJson((v) => !v)}
          className="text-xs text-gray-500 hover:text-gray-300 transition-colors ml-auto"
        >
          {showJson ? "Hide JSON" : "Show JSON"}
        </button>
      </div>

      {/* Error banner */}
      {lastError && (
        <div className="text-xs text-red-400 bg-red-950 border border-red-800 rounded px-3 py-1.5">
          {lastError}
        </div>
      )}

      {showJson && (
        <pre className="text-xs text-green-400 bg-black rounded p-3 overflow-auto max-h-48 leading-relaxed">
          {JSON.stringify(snapshot, null, 2)}
        </pre>
      )}
    </div>
  );
}
