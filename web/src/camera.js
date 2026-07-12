const LERP = 0.15;          // smoothing (0..1 per frame); tune by feel
const DOWN_LEAD = 60;       // cells: bias the view downward so you see what you descend into
const DEADZONE_X = 40, DEADZONE_Y = 30; // cells: target doesn't move for small avatar moves

// Smoothed follow-camera: x,y are the clamped top-left of the visible world window (in world
// cells). update() lerps toward the avatar with a downward lead (see more of what's below you)
// and a dead-zone (small avatar jitter doesn't chase the camera), then clamps to world edges.
export function makeCamera(viewW, viewH, worldW, worldH) {
  const clampX = v => Math.max(0, Math.min(worldW - viewW, v));
  const clampY = v => Math.max(0, Math.min(worldH - viewH, v));
  const cam = { x: 0, y: 0 };
  cam.update = (acx, acy) => {
    // desired top-left so the avatar sits at (viewW/2, viewH/2 - DOWN_LEAD)
    let tx = acx - viewW / 2;
    let ty = acy - (viewH / 2 - DOWN_LEAD);
    // dead-zone: only chase once the avatar has drifted past the box from current center
    const cx = cam.x + viewW / 2, cy = cam.y + (viewH / 2 - DOWN_LEAD);
    if (Math.abs(acx - cx) < DEADZONE_X) tx = cam.x;
    if (Math.abs(acy - cy) < DEADZONE_Y) ty = cam.y;
    cam.x = clampX(cam.x + (tx - cam.x) * LERP);
    cam.y = clampY(cam.y + (ty - cam.y) * LERP);
  };
  return cam;
}
