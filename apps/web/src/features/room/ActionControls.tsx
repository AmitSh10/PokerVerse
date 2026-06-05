import { useState } from "react";
import type { GameSnapshot, SeatIndex } from "../../types/api";
import { commands, type RoomCommand } from "../../api/commands";

interface ActionControlsProps {
  snapshot: GameSnapshot;
  viewerSeat: SeatIndex | null;
  onCommand: (cmd: RoomCommand) => Promise<void>;
}

export function ActionControls({ snapshot, viewerSeat, onCommand }: ActionControlsProps) {
  // All hooks must be at the top — no early returns before this line
  const [betAmount, setBetAmount] = useState("");
  const [pending, setPending] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const hand = snapshot.hand;
  const BETTING_PHASES = ["PreFlop", "Flop", "Turn", "River"];

  if (!hand || viewerSeat === null) return null;
  if (!BETTING_PHASES.includes(hand.phase)) return null;
  if (hand.acting_seat !== viewerSeat) {
    return (
      <div className="text-center text-gray-500 text-sm py-3">
        Waiting for seat {hand.acting_seat ?? "?"} to act…
      </div>
    );
  }

  const player = snapshot.players.find((p) => p.seat === viewerSeat);
  const roundContrib = hand.contributions.find((c) => c.seat === viewerSeat)?.round ?? 0;
  const canCheck = hand.current_bet === 0 || roundContrib >= hand.current_bet;
  const callAmount = hand.current_bet - roundContrib;
  const parsedAmount = parseInt(betAmount, 10);
  const hasBet = hand.current_bet > 0;

  // Pot size after calling (basis for pot-relative sizing)
  const effectivePot = hand.pot + callAmount;

  const quickSizes = [
    { label: "¼ pot", amount: Math.max(1, Math.round(effectivePot / 4)) },
    { label: "½ pot", amount: Math.max(1, Math.round(effectivePot / 2)) },
    { label: "¾ pot", amount: Math.max(1, Math.round((effectivePot * 3) / 4)) },
    { label: "Pot",   amount: effectivePot },
  ];

  const act = async (cmd: RoomCommand) => {
    if (pending) return;
    setActionError(null);
    setPending(true);
    try {
      await onCommand(cmd);
      setBetAmount("");
    } catch (e) {
      setActionError((e as Error).message);
    } finally {
      setPending(false);
    }
  };

  const btn = (label: string, cmd: RoomCommand, variant = "default") => {
    const base = "px-4 py-2 rounded font-semibold text-sm transition-colors disabled:opacity-40";
    const styles: Record<string, string> = {
      default: "bg-gray-700 hover:bg-gray-600 text-white",
      danger:  "bg-red-800 hover:bg-red-700 text-white",
      primary: "bg-blue-700 hover:bg-blue-600 text-white",
      success: "bg-green-700 hover:bg-green-600 text-white",
    };
    return (
      <button disabled={pending} className={`${base} ${styles[variant]}`} onClick={() => act(cmd)}>
        {label}
      </button>
    );
  };

  return (
    <div className="bg-gray-900 border-t border-gray-700 p-4">
      {actionError && (
        <p className="text-red-400 text-xs text-center mb-2">{actionError}</p>
      )}
      <p className="text-xs text-gray-500 mb-3 text-center">
        Your turn — seat {viewerSeat}
        {player && <span className="ml-2 text-gray-400">({player.stack} chips)</span>}
        {effectivePot > 0 && <span className="ml-2 text-gray-500">· pot {effectivePot}</span>}
      </p>

      <div className="flex flex-wrap gap-2 justify-center items-center">
        {/* Primary actions */}
        {btn("Fold", commands.fold(viewerSeat), "danger")}
        {canCheck
          ? btn("Check", commands.check(viewerSeat))
          : btn(`Call ${callAmount}`, commands.call(viewerSeat), "primary")}

        {/* Bet / raise sizing */}
        <div className="flex flex-col gap-1.5 items-start">
          {/* Quick-size buttons */}
          <div className="flex gap-1">
            {quickSizes.map(({ label, amount }) => (
              <button
                key={label}
                disabled={pending || amount > (player?.stack ?? 0)}
                onClick={() => setBetAmount(String(amount))}
                className={`
                  text-xs px-2 py-1 rounded border transition-colors disabled:opacity-30
                  ${betAmount === String(amount)
                    ? "bg-yellow-700 border-yellow-500 text-white"
                    : "bg-gray-800 border-gray-600 text-gray-300 hover:bg-gray-700"}
                `}
              >
                {label}
              </button>
            ))}
          </div>

          {/* Manual input + submit */}
          <div className="flex gap-1 items-center">
            <input
              type="number"
              placeholder="Amount"
              value={betAmount}
              onChange={(e) => setBetAmount(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !isNaN(parsedAmount)) {
                  act(hasBet ? commands.raise(viewerSeat, parsedAmount) : commands.bet(viewerSeat, parsedAmount));
                }
              }}
              className="bg-gray-700 text-white text-sm px-3 py-2 rounded border border-gray-600 w-24"
            />
            {btn(
              hasBet ? "Raise" : "Bet",
              hasBet
                ? commands.raise(viewerSeat, parsedAmount)
                : commands.bet(viewerSeat, parsedAmount),
              "success",
            )}
          </div>
        </div>

        {btn("All-In", commands.allIn(viewerSeat), "danger")}
      </div>
    </div>
  );
}
