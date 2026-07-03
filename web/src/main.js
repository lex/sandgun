import init, { WasmWorld } from './pkg/sandgun_wasm.js';
import { initGL, blit } from './renderer.js';
import { seedScene } from './scene.js';

const W = 640, H = 384;

const wasm = await init();
const world = new WasmWorld(W, H);
seedScene(world, W, H);

const ctx = initGL(document.getElementById('view'), W, H);
window.sandgun = { world, wasm }; // console poking

function frame() {
  world.step();
  world.render();
  // wasm memory growth invalidates old buffers — take a fresh view every frame
  const rgba = new Uint8Array(wasm.memory.buffer, world.rgba_ptr(), world.rgba_len());
  blit(ctx, rgba);
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
