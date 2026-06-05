import { useEffect, useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../api/client";
import { commands, type RoomCommand } from "../../api/commands";
import { useRoomSocket } from "../../hooks/useRoomSocket";
import { useRoomStore } from "../../stores/roomStore";
import { ConnectionBadge } from "../../components/ConnectionBadge";
import { TableView } from "./TableView";
import { ActionControls } from "./ActionControls";
import { EventLog } from "./EventLog";
import { DebugPanel } from "./DebugPanel";
import type { GameEvent, GameSnapshot, SeatIndex } from "../../types/api";

export default function RoomPage() {
  const { roomId: roomIdStr } = useParams<{ roomId: string }>();
  const roomId = Number(roomIdStr);
  const navigate = useNavigate();
  const qc = useQueryClient();

  const { viewerSeat, setViewerSeat } = useRoomStore();
  // WS is receive-only: broadcasts from other players arrive here.
  const { status, lastResult } = useRoomSocket(roomId);

  const [snapshot, setSnapshot] = useState<GameSnapshot | null>(null);
  const [events, setEvents] = useState<GameEvent[]>([]);
  const [lastError, setLastError] = useState<string | null>(null);

  // Initial HTTP load
  const { data: roomDetails, error: roomError } = useQuery({
    queryKey: ["room", roomId],
    queryFn: () => api.getRoom(roomId),
    enabled: !Number.isNaN(roomId),
  });

  // Seed snapshot from HTTP on first load
  useEffect(() => {
    if (roomDetails && !snapshot) setSnapshot(roomDetails.snapshot);
  }, [roomDetails, snapshot]);

  // Refresh private snapshot when viewer seat is chosen
  useEffect(() => {
    if (viewerSeat === null || !snapshot) return;
    api
      .getSeatSnapshot(roomId, viewerSeat)
      .then((s) => setSnapshot(s))
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [viewerSeat]);

  // Apply WS broadcasts (other players' actions or server-pushed updates)
  useEffect(() => {
    if (!lastResult) return;
    setSnapshot(lastResult.snapshot);
    setEvents((prev) => [...prev, ...lastResult.events]);
    setLastError(null);
  }, [lastResult]);

  // All commands go via HTTP for reliable synchronous confirmation.
  // The WS keeps other connected clients in sync automatically.
  const handleCommand = async (cmd: RoomCommand): Promise<void> => {
    setLastError(null);
    try {
      const result = await api.sendCommand(roomId, cmd);
      setSnapshot(result.snapshot);
      setEvents((prev) => [...prev, ...result.events]);
    } catch (e) {
      setLastError((e as Error).message);
    }
  };

  // Runs the full sequence to reach PreFlop in one click:
  // StartHand → AdvancePhase (PostingBlinds) → PostBlinds → AdvancePhase (PreFlop)
  const handleQuickStart = async (dealerSeat: SeatIndex): Promise<void> => {
    setLastError(null);
    try {
      for (const cmd of [
        commands.startHand(dealerSeat),
        commands.advanceHandPhase(),
        commands.postBlinds(),
        commands.advanceHandPhase(),
      ]) {
        const result = await api.sendCommand(roomId, cmd);
        setSnapshot(result.snapshot);
        setEvents((prev) => [...prev, ...result.events]);
      }
    } catch (e) {
      setLastError((e as Error).message);
    }
  };

  const handleCloseRoom = async () => {
    try {
      await api.deleteRoom(roomId);
      navigate("/");
    } catch (e) {
      setLastError((e as Error).message);
    }
  };

  const handleRefresh = () => {
    qc.invalidateQueries({ queryKey: ["room", roomId] });
    const fetch =
      viewerSeat !== null
        ? api.getSeatSnapshot(roomId, viewerSeat)
        : api.getRoom(roomId).then((d) => d.snapshot);
    fetch.then(setSnapshot).catch(() => {});
  };

  if (roomError) {
    return (
      <div className="min-h-screen bg-gray-950 flex items-center justify-center">
        <div className="text-center">
          <p className="text-red-400 mb-4">Room not found or server unreachable.</p>
          <button
            onClick={() => navigate("/")}
            className="text-blue-400 hover:text-blue-300 text-sm"
          >
            ← Back to lobby
          </button>
        </div>
      </div>
    );
  }

  const maxSeats = roomDetails?.summary.table_config.max_seats ?? 6;

  return (
    <div className="min-h-screen bg-gray-950 flex flex-col">
      {/* Top bar */}
      <header className="bg-gray-900 border-b border-gray-800 px-4 py-2 flex items-center gap-4">
        <button
          onClick={() => navigate("/")}
          className="text-gray-500 hover:text-gray-300 text-sm transition-colors"
        >
          ← Lobby
        </button>
        <span className="text-white font-semibold">Room #{roomId}</span>
        {roomDetails && (
          <span className="text-gray-500 text-xs">
            {roomDetails.summary.table_config.small_blind}/
            {roomDetails.summary.table_config.big_blind} blinds · {maxSeats}-max
          </span>
        )}
        <div className="ml-auto flex items-center gap-3">
          {viewerSeat !== null && (
            <span className="text-blue-400 text-xs">Viewing as seat {viewerSeat}</span>
          )}
          <ConnectionBadge status={status} />
          <button
            onClick={handleCloseRoom}
            className="text-gray-600 hover:text-red-400 text-xs transition-colors border border-gray-700 hover:border-red-800 px-2 py-1 rounded"
            title="Close room"
          >
            Close room
          </button>
        </div>
      </header>

      {/* Main content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Table area */}
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="flex-1 p-4 overflow-auto">
            {snapshot ? (
              <TableView
                snapshot={snapshot}
                maxSeats={maxSeats}
                viewerSeat={viewerSeat}
                onViewerSeatChange={setViewerSeat}
                onCommand={handleCommand}
              />
            ) : (
              <div className="flex items-center justify-center h-full">
                <p className="text-gray-600">Loading table…</p>
              </div>
            )}
          </div>

          {snapshot && (
            <ActionControls
              snapshot={snapshot}
              viewerSeat={viewerSeat}
              onCommand={handleCommand}
            />
          )}

          {snapshot && (
            <DebugPanel
              snapshot={snapshot}
              viewerSeat={viewerSeat}
              onViewerSeatChange={setViewerSeat}
              onCommand={handleCommand}
              onQuickStart={handleQuickStart}
              onRefresh={handleRefresh}
              lastError={lastError}
            />
          )}
        </div>

        {/* Event log sidebar */}
        <aside className="w-64 border-l border-gray-800 bg-gray-900 flex flex-col overflow-hidden shrink-0">
          <EventLog events={events} />
        </aside>
      </div>
    </div>
  );
}
