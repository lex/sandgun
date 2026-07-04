import init, { WasmWorld } from './pkg/sandgun_wasm.js';
import { initGL, blit } from './renderer.js';
import { attachInput, applyInput } from './input.js';
import { attachGun, applyGun } from './gun.js';
import { drawOverlay } from './overlay.js';
import { loadParams } from './params.js';

const W = 640, H = 384;

const wasm = await init();
const world = new WasmWorld(W, H);
world.generate((Math.random() * 0xFFFFFFFF) >>> 0);
await loadParams(world);
world.spawn_avatar(W / 2, 4);

const ctx = initGL(document.getElementById('view'), W, H);
const input = attachInput(document.getElementById('view'), W, H);
input.onReloadParams = () => loadParams(world);
const gun = attachGun(document.getElementById('view'), W, H);
const octx = document.getElementById('overlay').getContext('2d');

// Fixed-timestep sim: step at a constant 60 Hz regardless of display refresh so
// game speed doesn't scale with monitor Hz. Render still runs every rAF.
const TICK_HZ = 60;
const TICK_MS = 1000 / TICK_HZ;
const MAX_STEPS = 4; // backlog cap — drop excess to avoid a spiral of death after a stall
let fps = 60, last = performance.now(), acc = 0;
window.sandgun = { world, wasm, input, gun }; // console poking

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

  world.render();
  // wasm memory growth invalidates old buffers — take a fresh view every frame
  const rgba = new Uint8Array(wasm.memory.buffer, world.rgba_ptr(), world.rgba_len());
  blit(ctx, rgba);
  fps = fps * 0.95 + (1000 / Math.max(1, dt)) * 0.05;
  drawOverlay(octx, world, wasm, input, fps, gun);
  window.sandgun.fps = fps; // measurement hook (M0 task 9 acceptance)
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
