import type { RoomCommandResult, ApiErrorBody, WsServerMessage } from "../types/api";
import type { RoomCommand } from "../api/commands";

const WS_BASE_URL = import.meta.env.VITE_WS_BASE_URL ?? "ws://127.0.0.1:3000";

type MessageHandler = (msg: WsServerMessage) => void;
type StatusHandler = (status: SocketStatus) => void;

export type SocketStatus = "connecting" | "connected" | "reconnecting" | "closed";

const RECONNECT_DELAY_MS = 2_000;
const MAX_RECONNECT_ATTEMPTS = 10;

export class RoomSocket {
  private ws: WebSocket | null = null;
  private messageHandlers = new Set<MessageHandler>();
  private statusHandlers = new Set<StatusHandler>();
  private reconnectAttempts = 0;
  private closed = false;

  private readonly roomId: number;

  constructor(roomId: number) {
    this.roomId = roomId;
  }

  connect(): void {
    if (this.ws) return;
    this.closed = false;
    this.open();
  }

  private open(): void {
    const url = `${WS_BASE_URL}/rooms/${this.roomId}/ws`;
    this.ws = new WebSocket(url);
    this.emit("connecting");

    this.ws.onopen = () => {
      this.reconnectAttempts = 0;
      this.emit("connected");
    };

    this.ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(event.data as string) as WsServerMessage;
        this.messageHandlers.forEach((h) => h(msg));
      } catch {
        // ignore malformed frames
      }
    };

    this.ws.onclose = () => {
      this.ws = null;
      if (!this.closed) this.scheduleReconnect();
    };

    this.ws.onerror = () => {
      this.ws?.close();
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
    this.ws?.close();
    this.ws = null;
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
