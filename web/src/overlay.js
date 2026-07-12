import { MATERIAL_COLOR, PROJECTILE_COLOR, AVATAR_COLOR } from './scene.js';

// Chunk size in world cells — must match sandgun-core's `world::CHUNK` (not exposed to wasm).
const CHUNK = 64;

// M1d task 3: particles/projectiles/avatar are no longer stamped into the persistent world
// texture (that would leave trails once an entity moves on) -- they're drawn fresh here every
// frame instead, on the 2D overlay canvas. Must run regardless of the `g` debug toggle: entities
// are gameplay-visible, not a debug aid. World is rendered 1:1 (see the chunk-box comment below),
// so screen pos is just world pos minus the camera's world-cell top-left -- no extra scale.
function drawEntities(octx, world, cam) {
  const { width: w, height: h } = octx.canvas;
  const camX = cam?.x ?? 0, camY = cam?.y ?? 0;

  const particles = world.particles_xy(); // [x0,y0,m0, x1,y1,m1, ...]
  for (let i = 0; i < particles.length; i += 3) {
    const sx = Math.floor(particles[i] - camX);
    const sy = Math.floor(particles[i + 1] - camY);
    if (sx < 0 || sy < 0 || sx >= w || sy >= h) continue;
    octx.fillStyle = MATERIAL_COLOR[particles[i + 2] | 0] ?? '#fff';
    octx.fillRect(sx, sy, 1, 1);
  }

  const projectiles = world.projectiles_xy(); // [x0,y0, x1,y1, ...]
  octx.fillStyle = PROJECTILE_COLOR;
  for (let i = 0; i < projectiles.length; i += 2) {
    const sx = Math.floor(projectiles[i] - camX);
    const sy = Math.floor(projectiles[i + 1] - camY);
    if (sx < 0 || sy < 0 || sx >= w || sy >= h) continue;
    octx.fillRect(sx, sy, 1, 1);
  }

  const a = world.avatar_xywh();
  if (a) {
    const [ax, ay, aw, ah] = a;
    const sx = Math.floor(ax - camX), sy = Math.floor(ay - camY);
    if (sx + aw > 0 && sy + ah > 0 && sx < w && sy < h) {
      octx.fillStyle = AVATAR_COLOR;
      octx.fillRect(sx, sy, aw, ah);
    }
  }
}

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
  drawEntities(octx, world, cam);
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
