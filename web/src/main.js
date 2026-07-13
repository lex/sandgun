import init, { WasmWorld } from './pkg/sandgun_wasm.js';
import { initGL, uploadDirtyChunks, drawCamera, seedEmission, propagate, drawLit } from './renderer.js';
import { attachInput, applyInput } from './input.js';
import { attachGun, applyGun } from './gun.js';
import { drawOverlay } from './overlay.js';
import { loadParams } from './params.js';
import { makeCamera } from './camera.js';

const WORLD_W = 1024, WORLD_H = 2048;
const VIEW_W = 640, VIEW_H = 384;

const wasm = await init();
const world = new WasmWorld(WORLD_W, WORLD_H);
world.generate((Math.random() * 0xFFFFFFFF) >>> 0);
await loadParams(world);
world.spawn_avatar(WORLD_W / 2, 4);

const cam = makeCamera(VIEW_W, VIEW_H, WORLD_W, WORLD_H);
const view = document.getElementById('view');
const ctx = initGL(view, WORLD_W, WORLD_H, VIEW_W, VIEW_H);
const input = attachInput(view, VIEW_W, VIEW_H, cam);
input.onReloadParams = () => loadParams(world);
const gun = attachGun(view, VIEW_W, VIEW_H, cam);
const octx = document.getElementById('overlay').getContext('2d');

// Fixed-timestep sim: step at a constant 60 Hz regardless of display refresh so
// game speed doesn't scale with monitor Hz. Render still runs every rAF.
const TICK_HZ = 60;
const TICK_MS = 1000 / TICK_HZ;
const MAX_STEPS = 4; // backlog cap — drop excess to avoid a spiral of death after a stall
let fps = 60, last = performance.now(), acc = 0;
window.sandgun = { world, wasm, input, gun, cam }; // console poking
window.sandgun.gfx = ctx; // lighting resources reachable from tests

// One simulation tick: latest avatar/gun intent, then advance the world.
function stepOnce() {
  world.set_avatar_input(input.left, input.right, input.jump);
  applyGun(gun, world);
  world.step();
}

function frame() {
  applyInput(input, world); // per-frame UI: painting, regen, param reload
  const now = performance.now();
  let dt = now - last;
  last = now;
  if (dt > 250) dt = 250; // clamp huge gaps (tab was backgrounded)

  if (input.capTicks) {
    acc += dt;
    let n = 0;
    while (acc >= TICK_MS && n < MAX_STEPS) { stepOnce(); acc -= TICK_MS; n++; }
    if (n === MAX_STEPS) acc = 0; // fell behind — abandon the backlog
  } else {
    stepOnce(); // uncapped: one step per rAF (sim speed tracks refresh rate)
    acc = 0;
  }

  world.render(); // core rewrites RGBA only for chunks it marked render-dirty (M1d task 3)
  // wasm memory growth invalidates old buffers — take fresh views every frame
  const rgba = new Uint8Array(wasm.memory.buffer, world.rgba_ptr(), world.rgba_len());
  const dirty = new Uint8Array(wasm.memory.buffer, world.render_dirty_ptr(), world.render_dirty_len());
  const uploaded = uploadDirtyChunks(ctx, rgba, dirty); // settled world uploads ~zero chunks
  world.clear_render_dirty();
  const c = world.avatar_center();
  if (c) cam.update(c[0], c[1]);
  const cx = Math.floor(cam.x), cy = Math.floor(cam.y);
  if (input.lightingOn !== false) {
    seedEmission(ctx, cx, cy);
    propagate(ctx, cx, cy, 24);
    // avatar centre -> TOP-DOWN screen pixels; drawLit flips Y internally
    let px = -1, py = -1;
    if (c) { px = c[0] - cx; py = c[1] - cy; }
    drawLit(ctx, cx, cy, { playerX: px, playerY: py, playerRadius: 90 });
  } else {
    drawCamera(ctx, cx, cy); // camera pan alone uploads nothing
  }
  fps = fps * 0.95 + (1000 / Math.max(1, dt)) * 0.05;
  drawOverlay(octx, world, wasm, input, fps, gun, cam, uploaded);
  window.sandgun.fps = fps; // measurement hook (M0 task 9 acceptance)
  window.sandgun.uploadedChunks = uploaded; // measurement hook (M1d task 3/4: dirty-chunk count)
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
