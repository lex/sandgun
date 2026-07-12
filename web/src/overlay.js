// Chunk size in world cells — must match sandgun-core's `world::CHUNK` (not exposed to wasm).
const CHUNK = 64;

export function drawOverlay(octx, world, wasm, input, fps, gun, cam) {
  const { width: w, height: h } = octx.canvas;
  octx.clearRect(0, 0, w, h);
  octx.font = '10px monospace';
  octx.fillStyle = '#9f9';
  const rate = input.capTicks ? '60hz' : 'uncap';
  let growth = '';
  if (input.debug) {
    growth = ` · colonies ${world.colony_count()} · tips ${world.tip_count()} · mush ${world.mushroom_count()} · pool ${world.max_colony_pool()}`;
  }
  octx.fillText(`${fps.toFixed(0)} fps · ${rate} · ${input.status}${gun ? ` · ${gun.status}` : ''}${input.debug ? ` · ${world.cells_processed()} cells${growth}` : ''}`, 6, 12);
  if (!input.debug) return;
  const active = new Uint8Array(wasm.memory.buffer, world.active_ptr(), world.active_len());
  octx.strokeStyle = 'rgba(255, 60, 60, 0.8)';
  const camX = cam?.x ?? 0, camY = cam?.y ?? 0;
  const chunksX = world.chunks_x(), chunksY = world.chunks_y();
  // World is rendered 1:1 (crisp pixels), so a chunk box is CHUNK screen px once offset by
  // the camera's world-cell top-left; cull chunks fully outside the visible window.
  for (let cy = 0; cy < chunksY; cy++) {
    const sy = cy * CHUNK - camY;
    if (sy + CHUNK <= 0 || sy >= h) continue;
    for (let cx = 0; cx < chunksX; cx++) {
      const sx = cx * CHUNK - camX;
      if (sx + CHUNK <= 0 || sx >= w) continue;
      if (active[cy * chunksX + cx]) {
        octx.strokeRect(sx + 0.5, sy + 0.5, CHUNK - 1, CHUNK - 1);
      }
    }
  }
}
