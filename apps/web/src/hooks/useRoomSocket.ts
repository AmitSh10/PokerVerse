import { useEffect, useRef, useState } from "react";
import { RoomSocket, type SocketStatus } from "../ws/roomSocket";
import type { RoomCommandResult } from "../types/api";
import type { RoomCommand } from "../api/commands";

export interface UseRoomSocketResult {
  send: (cmd: RoomCommand) => void;
  status: SocketStatus;
  lastResult: RoomCommandResult | null;
}

export function useRoomSocket(roomId: number): UseRoomSocketResult {
  const socketRef = useRef<RoomSocket | null>(null);
  const [status, setStatus] = useState<SocketStatus>("connecting");
  const [lastResult, setLastResult] = useState<RoomCommandResult | null>(null);

  useEffect(() => {
    const socket = new RoomSocket(roomId);
    socketRef.current = socket;

    const unsubStatus = socket.onStatus(setStatus);
    const unsubMessage = socket.onMessage((msg) => {
      if ("CommandResult" in msg) setLastResult(msg.CommandResult);
    });

    socket.connect();

    return () => {
      unsubStatus();
      unsubMessage();
      socket.close();
      socketRef.current = null;
    };
  }, [roomId]);

  const send = (cmd: RoomCommand) => socketRef.current?.send(cmd);

  return { send, status, lastResult };
}
