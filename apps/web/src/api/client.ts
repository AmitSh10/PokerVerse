import type {
  CreateRoomRequest,
  CreateRoomResponse,
  GameSnapshot,
  HealthResponse,
  ListRoomsResponse,
  RoomCommandResult,
  RoomDetailsResponse,
  RoomId,
  SeatIndex,
} from "../types/api";
import type { RoomCommand } from "./commands";

const BASE_URL = import.meta.env.VITE_API_BASE_URL ?? "http://127.0.0.1:3000";

class ApiError extends Error {
  readonly code: string;
  readonly status: number;

  constructor(code: string, message: string, status: number) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.status = status;
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE_URL}${path}`, {
    headers: { "Content-Type": "application/json", ...init?.headers },
    ...init,
  });

  if (!res.ok) {
    let code = "unknown_error";
    let message = res.statusText;
    try {
      const body = await res.json();
      code = body.code ?? code;
      message = body.error ?? message;
    } catch {
      // ignore parse failure — use defaults above
    }
    throw new ApiError(code, message, res.status);
  }

  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

export const api = {
  health(): Promise<HealthResponse> {
    return request("/health");
  },

  listRooms(): Promise<ListRoomsResponse> {
    return request("/rooms");
  },

  createRoom(body: CreateRoomRequest): Promise<CreateRoomResponse> {
    return request("/rooms", { method: "POST", body: JSON.stringify(body) });
  },

  getRoom(roomId: number): Promise<RoomDetailsResponse> {
    return request(`/rooms/${roomId}`);
  },

  deleteRoom(roomId: number): Promise<void> {
    return request(`/rooms/${roomId}`, { method: "DELETE" });
  },

  getSeatSnapshot(roomId: number, seat: number): Promise<GameSnapshot> {
    return request(`/rooms/${roomId}/seats/${seat}/snapshot`);
  },

  sendCommand(roomId: number, command: RoomCommand): Promise<RoomCommandResult> {
    return request(`/rooms/${roomId}/commands`, {
      method: "POST",
      body: JSON.stringify(command),
    });
  },
};

export { ApiError };
export type { RoomId, SeatIndex };
