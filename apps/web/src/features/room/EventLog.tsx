import { useEffect, useRef } from "react";
import type { GameEvent } from "../../types/api";
import { formatEvent } from "../../lib/formatEvent";

export function EventLog({ events }: { events: GameEvent[] }) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [events.length]);

  return (
    <div className="flex flex-col h-full">
      <h3 className="text-xs font-semibold text-gray-500 uppercase tracking-wider px-3 pt-3 pb-2 border-b border-gray-700">
        Event Log
      </h3>
      <div className="flex-1 overflow-y-auto px-3 py-2 space-y-0.5">
        {events.length === 0 && (
          <p className="text-gray-600 text-xs">No events yet.</p>
        )}
        {events.map((event, i) => (
          <div key={i} className="text-xs text-gray-300 leading-relaxed">
            <span className="text-gray-600 mr-1.5 select-none">{i + 1}.</span>
            {formatEvent(event)}
          </div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
