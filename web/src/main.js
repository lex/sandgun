import init, { WasmWorld } from './pkg/sandgun_wasm.js';
import { initGL, blit } from './renderer.js';
import { attachInput, applyInput } from './input.js';
import { drawOverlay } from './overlay.js';
import { loadParams } from './params.js';

const W = 640, H = 384;

const wasm = await init();
const world = new WasmWorld(W, H);
world.generate((Math.random() * 0xFFFFFFFF) >>> 0);
await loadParams(world);

const ctx = initGL(document.getElementById('view'), W, H);
const input = attachInput(document.getElementById('view'), W, H);
input.onReloadParams = () => loadParams(world);
const octx = document.getElementById('overlay').getContext('2d');
let fps = 60, last = performance.now();
window.sandgun = { world, wasm, input }; // console poking

function frame() {
  applyInput(input, world);
  world.step();
  world.render();
  // wasm memory growth invalidates old buffers — take a fresh view every frame
  const rgba = new Uint8Array(wasm.memory.buffer, world.rgba_ptr(), world.rgba_len());
  blit(ctx, rgba);
  const now = performance.now();
  fps = fps * 0.95 + (1000 / Math.max(1, now - last)) * 0.05;
  last = now;
  drawOverlay(octx, world, wasm, input, fps);
  window.sandgun.fps = fps; // measurement hook (M0 task 9 acceptance)
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
