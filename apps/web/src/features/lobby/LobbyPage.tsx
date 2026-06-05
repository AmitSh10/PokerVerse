import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { api } from "../../api/client";
import type { CreateRoomRequest, RoomSummary } from "../../types/api";

const DEFAULT_CONFIG: CreateRoomRequest["table_config"] = {
  max_seats: 6,
  small_blind: 5,
  big_blind: 10,
  min_buy_in: 100,
  max_buy_in: 1000,
};

function RoomRow({
  room,
  onJoin,
  onClose,
}: {
  room: RoomSummary;
  onJoin: () => void;
  onClose: () => void;
}) {
  const [confirming, setConfirming] = useState(false);

  return (
    <div className="bg-gray-800 border border-gray-700 rounded-lg p-4 flex items-center justify-between">
      <div className="flex items-center gap-4">
        <span className="text-lg font-semibold text-white">Room #{room.id}</span>
        <span className="text-gray-400 text-sm">
          {room.seated_player_count}/{room.seat_count} seats
        </span>
        <span className="text-gray-500 text-xs">
          {room.table_config.small_blind}/{room.table_config.big_blind} blinds
        </span>
        {room.active_phase && (
          <span className="bg-yellow-900 text-yellow-300 text-xs px-2 py-0.5 rounded">
            {room.active_phase}
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        <button
          onClick={onJoin}
          className="bg-blue-700 hover:bg-blue-600 active:bg-blue-800 text-white px-4 py-1.5 rounded text-sm font-medium transition-colors"
        >
          Join
        </button>
        {confirming ? (
          <>
            <span className="text-gray-400 text-xs">Close room?</span>
            <button
              onClick={() => { onClose(); setConfirming(false); }}
              className="bg-red-700 hover:bg-red-600 text-white px-3 py-1.5 rounded text-xs font-medium transition-colors"
            >
              Yes
            </button>
            <button
              onClick={() => setConfirming(false)}
              className="bg-gray-700 hover:bg-gray-600 text-white px-3 py-1.5 rounded text-xs transition-colors"
            >
              No
            </button>
          </>
        ) : (
          <button
            onClick={() => setConfirming(true)}
            className="text-gray-500 hover:text-red-400 text-xs px-2 py-1.5 rounded transition-colors"
            title="Close room"
          >
            ✕
          </button>
        )}
      </div>
    </div>
  );
}

export default function LobbyPage() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [newRoomId, setNewRoomId] = useState("1");

  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ["rooms"],
    queryFn: () => api.listRooms(),
    refetchInterval: 5000,
  });

  const createRoom = useMutation({
    mutationFn: (req: CreateRoomRequest) => api.createRoom(req),
    onSuccess: (res) => {
      qc.invalidateQueries({ queryKey: ["rooms"] });
      navigate(`/rooms/${res.id}`);
    },
  });

  const closeRoom = useMutation({
    mutationFn: (roomId: number) => api.deleteRoom(roomId),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["rooms"] }),
  });

  const handleCreate = () => {
    const id = parseInt(newRoomId, 10);
    if (!Number.isNaN(id) && id > 0) {
      createRoom.mutate({ id, table_config: DEFAULT_CONFIG });
    }
  };

  return (
    <div className="min-h-screen bg-gray-950 text-white p-8 max-w-3xl mx-auto">
      <div className="flex items-center justify-between mb-8">
        <h1 className="text-3xl font-bold text-yellow-400">PokerVerse</h1>
        <button
          onClick={() => refetch()}
          className="text-gray-500 hover:text-gray-300 text-sm transition-colors"
        >
          ↻ Refresh
        </button>
      </div>

      {/* Create room */}
      <div className="bg-gray-800 border border-gray-700 rounded-lg p-5 mb-8">
        <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-3">
          Create Room
        </h2>
        <div className="flex gap-3 items-center">
          <label className="text-sm text-gray-400">Room ID</label>
          <input
            type="number"
            min="1"
            value={newRoomId}
            onChange={(e) => setNewRoomId(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleCreate()}
            className="bg-gray-700 text-white px-3 py-1.5 rounded border border-gray-600 w-24 text-sm"
          />
          <button
            onClick={handleCreate}
            disabled={createRoom.isPending}
            className="bg-green-700 hover:bg-green-600 disabled:opacity-50 text-white px-4 py-1.5 rounded text-sm font-semibold transition-colors"
          >
            {createRoom.isPending ? "Creating…" : "Create & Join"}
          </button>
          {createRoom.error && (
            <span className="text-red-400 text-sm">{(createRoom.error as Error).message}</span>
          )}
        </div>
      </div>

      {/* Room list */}
      <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-3">
        Active Rooms
      </h2>

      {isLoading && <p className="text-gray-500 text-sm">Loading…</p>}
      {error && (
        <p className="text-red-400 text-sm">
          Cannot reach server — is the backend running?
        </p>
      )}

      <div className="flex flex-col gap-3">
        {data?.rooms.map((room) => (
          <RoomRow
            key={room.id}
            room={room}
            onJoin={() => navigate(`/rooms/${room.id}`)}
            onClose={() => closeRoom.mutate(room.id)}
          />
        ))}
        {data?.rooms.length === 0 && (
          <p className="text-gray-600 text-sm">No rooms yet.</p>
        )}
      </div>
    </div>
  );
}
