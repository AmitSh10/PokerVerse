import { useState } from "react";
import type { GameSnapshot, SeatIndex } from "../../types/api";
import { commands, type RoomCommand } from "../../api/commands";

interface DebugPanelProps {
  snapshot: GameSnapshot;
  maxSeats: number;
  viewerSeat: SeatIndex | null;
  onViewerSeatChange: (seat: SeatIndex | null) => void;
  onCommand: (cmd: RoomCommand) => void;
  onRefresh: () => void;
}

export function DebugPanel({
  snapshot,
  maxSeats,
  viewerSeat,
  onViewerSeatChange,
  onCommand,
  onRefresh,
}: DebugPanelProps) {
  const [showJson, setShowJson] = useState(false);
  const [dealerSeat, setDealerSeat] = useState("0");

  const shortcut = (label: string, cmd: RoomCommand) => (
    <button
      onClick={() => onCommand(cmd)}
      className="bg-gray-700 hover:bg-gray-600 text-gray-200 text-xs px-3 py-1.5 rounded transition-colors"
    >
      {label}
    </button>
  );

  return (
    <div className="bg-gray-950 border-t border-gray-800 p-3">
      <div className="flex flex-wrap gap-2 items-center">
        {/* Viewer seat selector */}
        <div className="flex items-center gap-1.5">
          <span className="text-xs text-gray-500">View as seat</span>
          <select
            value={viewerSeat ?? ""}
            onChange={(e) => onViewerSeatChange(e.target.value === "" ? null : Number(e.target.value))}
            className="bg-gray-700 text-white text-xs px-2 py-1 rounded border border-gray-600"
          >
            <option value="">— spectate —</option>
            {Array.from({ length: maxSeats }, (_, i) => (
              <option key={i} value={i}>Seat {i}</option>
            ))}
          </select>
        </div>

        <span className="text-gray-700">|</span>

        {/* Phase shortcuts */}
        <div className="flex items-center gap-1.5">
          <span className="text-xs text-gray-500">Dealer</span>
          <select
            value={dealerSeat}
            onChange={(e) => setDealerSeat(e.target.value)}
            className="bg-gray-700 text-white text-xs px-2 py-1 rounded border border-gray-600 w-20"
          >
            {Array.from({ length: maxSeats }, (_, i) => (
              <option key={i} value={i}>Seat {i}</option>
            ))}
          </select>
          {shortcut("Start Hand", commands.startHand(Number(dealerSeat)))}
        </div>
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

      {showJson && (
        <pre className="mt-3 text-xs text-green-400 bg-black rounded p-3 overflow-auto max-h-48 leading-relaxed">
          {JSON.stringify(snapshot, null, 2)}
        </pre>
      )}
    </div>
  );
}
