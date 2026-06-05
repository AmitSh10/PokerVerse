import type { GameSnapshot, SeatIndex } from "../../types/api";
import { getSeatPosition } from "../../lib/seatLayout";
import { PlayerSeat } from "./PlayerSeat";
import { BoardArea } from "./BoardArea";
import type { RoomCommand } from "../../api/commands";

interface TableViewProps {
  snapshot: GameSnapshot;
  maxSeats: number;
  viewerSeat: SeatIndex | null;
  onViewerSeatChange: (seat: SeatIndex | null) => void;
  onCommand: (cmd: RoomCommand) => void;
}

export function TableView({
  snapshot,
  maxSeats,
  viewerSeat,
  onViewerSeatChange,
  onCommand,
}: TableViewProps) {
  const seats = Array.from({ length: maxSeats }, (_, i) => i);
  const playerBySeat = new Map(snapshot.players.map((p) => [p.seat, p]));

  return (
    <div className="relative w-full" style={{ paddingBottom: "70%" }}>
      {/* Felt oval */}
      <div
        className="absolute inset-[8%] rounded-[50%] bg-green-800 border-4 border-yellow-800 flex items-center justify-center"
        style={{ boxShadow: "inset 0 4px 24px rgba(0,0,0,0.5), 0 0 0 6px #1c1410" }}
      >
        <BoardArea hand={snapshot.hand ?? null} />
      </div>

      {/* Seats */}
      {seats.map((i) => {
        const pos = getSeatPosition(i, maxSeats);
        return (
          <div
            key={i}
            className="absolute"
            style={{
              left: pos.left,
              top: pos.top,
              transform: "translate(-50%, -50%)",
            }}
          >
            <PlayerSeat
              seatIndex={i}
              player={playerBySeat.get(i)}
              hand={snapshot.hand ?? null}
              viewerSeat={viewerSeat}
              onViewerSeatChange={onViewerSeatChange}
              onCommand={onCommand}
            />
          </div>
        );
      })}
    </div>
  );
}
