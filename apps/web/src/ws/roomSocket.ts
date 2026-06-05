import type { RoomCommandResult, ApiErrorBody, WsServerMessage } from "../types/api";
import type { RoomCommand } from "../api/commands";

const WS_BASE_URL = import.meta.env.VITE_WS_BASE_URL ?? "ws://127.0.0.1:3000";

type MessageHandler = (msg: WsServerMessage) => void;
type StatusHandler = (status: SocketStatus) => void;

export type SocketStatus = "connecting" | "connected" | "reconnecting" | "closed";

const RECONNECT_DELAY_MS = 2_000;
const MAX_RECONNECT_ATTEMPTS = 10;

export class RoomSocket {
  private readonly roomId: number;
  private ws: WebSocket | null = null;
  private messageHandlers = new Set<MessageHandler>();
  private statusHandlers = new Set<StatusHandler>();
  private reconnectAttempts = 0;
  private closed = false;

  constructor(roomId: number) {
    this.roomId = roomId;
  }

  connect(): void {
    if (this.closed || this.ws) return;
    this.open();
  }

  private open(): void {
    const url = `${WS_BASE_URL}/rooms/${this.roomId}/ws`;
    const ws = new WebSocket(url);
    this.ws = ws;
    this.emit("connecting");

    ws.onopen = () => {
      if (this.ws !== ws) return; // superseded by a newer socket
      this.reconnectAttempts = 0;
      this.emit("connected");
    };

    ws.onmessage = (event) => {
      if (this.ws !== ws) return;
      try {
        const msg = JSON.parse(event.data as string) as WsServerMessage;
        this.messageHandlers.forEach((h) => h(msg));
      } catch {
        // ignore malformed frames
      }
    };

    ws.onclose = () => {
      if (this.ws !== ws) return; // already replaced or closed intentionally
      this.ws = null;
      if (!this.closed) this.scheduleReconnect();
    };

    ws.onerror = () => {
      ws.close();
    };
  }

  private scheduleReconnect(): void {
    if (this.reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
      this.emit("closed");
      return;
    }
    this.reconnectAttempts++;
    this.emit("reconnecting");
    setTimeout(() => {
      if (!this.closed) this.open();
    }, RECONNECT_DELAY_MS);
  }

  send(command: RoomCommand): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(command));
    }
  }

  close(): void {
    this.closed = true;
    const ws = this.ws;
    this.ws = null;
    // Only close if the socket has progressed past CONNECTING to avoid the
    // "WebSocket is closed before the connection is established" warning in
    // React StrictMode's double-invoke cleanup.
    if (ws && ws.readyState !== WebSocket.CONNECTING) {
      ws.close();
    } else if (ws) {
      // Still connecting — let onopen fire then close immediately
      ws.onopen = () => ws.close();
    }
    this.emit("closed");
  }

  onMessage(handler: MessageHandler): () => void {
    this.messageHandlers.add(handler);
    return () => this.messageHandlers.delete(handler);
  }

  onStatus(handler: StatusHandler): () => void {
    this.statusHandlers.add(handler);
    return () => this.statusHandlers.delete(handler);
  }

  private emit(status: SocketStatus): void {
    this.statusHandlers.forEach((h) => h(status));
  }

  get status(): SocketStatus {
    if (this.closed) return "closed";
    if (!this.ws) return "reconnecting";
    if (this.ws.readyState === WebSocket.CONNECTING) return "connecting";
    if (this.ws.readyState === WebSocket.OPEN) return "connected";
    return "reconnecting";
  }
}

export type { RoomCommandResult, ApiErrorBody };
