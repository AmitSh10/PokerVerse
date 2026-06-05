import type { SocketStatus } from "../ws/roomSocket";

const STYLES: Record<SocketStatus, string> = {
  connected: "bg-green-500",
  connecting: "bg-yellow-500 animate-pulse",
  reconnecting: "bg-orange-500 animate-pulse",
  closed: "bg-red-500",
};

export function ConnectionBadge({ status }: { status: SocketStatus }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-xs text-gray-300">
      <span className={`w-2 h-2 rounded-full ${STYLES[status]}`} />
      {status}
    </span>
  );
}
