import init, { WasmWorld } from './pkg/sandgun_wasm.js';
import { initGL, blit } from './renderer.js';
import { attachInput, applyInput } from './input.js';

const W = 640, H = 384;

const wasm = await init();
const world = new WasmWorld(W, H);
world.generate((Math.random() * 0xFFFFFFFF) >>> 0);

const ctx = initGL(document.getElementById('view'), W, H);
const input = attachInput(document.getElementById('view'), W, H);
window.sandgun = { world, wasm, input }; // console poking

function frame() {
  applyInput(input, world);
  world.step();
  world.render();
  // wasm memory growth invalidates old buffers — take a fresh view every frame
  const rgba = new Uint8Array(wasm.memory.buffer, world.rgba_ptr(), world.rgba_len());
  blit(ctx, rgba);
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
