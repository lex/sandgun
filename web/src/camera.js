const LERP = 0.2;           // smoothing (0..1 per frame); higher = tighter follow, less lag
const DOWN_LEAD = 60;       // cells: bias the view downward so you see what you descend into
const DEADZONE_X = 12, DEADZONE_Y = 24; // half-size (cells) of the box the avatar roams before the camera tracks

// Smoothed follow-camera: x,y are the clamped top-left of the visible world window (in world
// cells). update() keeps the avatar inside a dead-zone box around the view anchor (anchor is
// centered horizontally, DOWN_LEAD above center vertically); once the avatar reaches a box edge
// the camera tracks it CONTINUOUSLY (pinning it at the edge), then lerps + clamps to world edges.
export function makeCamera(viewW, viewH, worldW, worldH) {
  const clampX = v => Math.max(0, Math.min(worldW - viewW, v));
  const clampY = v => Math.max(0, Math.min(worldH - viewH, v));
  const cam = { x: 0, y: 0 };
  // Edge-following dead-zone: move the target only enough to keep `a` within +/-dz of the
  // anchor. This tracks the avatar 1:1 at the box edge (smooth steady scroll) instead of
  // freezing then snapping to center, which made steady walking scroll in laggy multi-cell jumps.
  const follow = (a, camPos, half, dz) => {
    const rel = a - (camPos + half); // avatar offset from the current view anchor
    if (rel > dz) return a - (half + dz); // avatar past the far edge -> pin it there
    if (rel < -dz) return a - (half - dz); // avatar past the near edge -> pin it there
    return camPos;                          // inside the box -> hold still
  };
  cam.update = (acx, acy) => {
    const tx = follow(acx, cam.x, viewW / 2, DEADZONE_X);
    const ty = follow(acy, cam.y, viewH / 2 - DOWN_LEAD, DEADZONE_Y);
    cam.x = clampX(cam.x + (tx - cam.x) * LERP);
    cam.y = clampY(cam.y + (ty - cam.y) * LERP);
  };
  return cam;
}
