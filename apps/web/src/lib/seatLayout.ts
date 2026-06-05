// Seat 0 is at the bottom-center (hero position). Remaining seats go clockwise.
export function getSeatPosition(
  seatIndex: number,
  maxSeats: number,
): { left: string; top: string } {
  // angle=PI/2 puts seat 0 at bottom, negative step = clockwise
  const angle = Math.PI / 2 - (seatIndex / maxSeats) * 2 * Math.PI;
  const rx = 40; // horizontal radius as % of container
  const ry = 36; // vertical radius as % of container
  return {
    left: `${50 + rx * Math.cos(angle)}%`,
    top: `${50 + ry * Math.sin(angle)}%`,
  };
}
