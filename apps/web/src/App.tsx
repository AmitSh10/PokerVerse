import { lazy, Suspense } from "react";
import { Routes, Route } from "react-router-dom";

const LobbyPage = lazy(() => import("./features/lobby/LobbyPage"));
const RoomPage = lazy(() => import("./features/room/RoomPage"));

export default function App() {
  return (
    <Suspense fallback={<div className="min-h-screen bg-gray-950" />}>
      <Routes>
        <Route path="/" element={<LobbyPage />} />
        <Route path="/rooms/:roomId" element={<RoomPage />} />
      </Routes>
    </Suspense>
  );
}
