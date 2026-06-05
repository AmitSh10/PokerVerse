import { create } from "zustand";
import type { SeatIndex } from "../types/api";

interface RoomStore {
  viewerSeat: SeatIndex | null;
  setViewerSeat: (seat: SeatIndex | null) => void;
  debugOpen: boolean;
  toggleDebug: () => void;
}

export const useRoomStore = create<RoomStore>((set) => ({
  viewerSeat: null,
  setViewerSeat: (seat) => set({ viewerSeat: seat }),
  debugOpen: false,
  toggleDebug: () => set((s) => ({ debugOpen: !s.debugOpen })),
}));
