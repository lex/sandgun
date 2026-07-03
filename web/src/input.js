import { M } from './scene.js';

const KEYS = { '1': M.SAND, '2': M.WATER, '3': M.OIL, '4': M.ROCK, '0': M.EMPTY, 'e': M.EMPTY };
const NAMES = ['erase', 'rock', 'sand', 'water', 'oil'];

export function attachInput(canvas, worldW, worldH) {
  const input = {
    down: false, x: 0, y: 0, px: null, py: null,
    material: M.SAND, radius: 4, debug: false, regen: false,
    get status() { return `${NAMES[this.material]} r${this.radius}`; },
  };
  const toWorld = (e) => {
    const r = canvas.getBoundingClientRect();
    input.x = Math.floor((e.clientX - r.left) / r.width * worldW);
    input.y = Math.floor((e.clientY - r.top) / r.height * worldH);
  };
  canvas.addEventListener('pointerdown', (e) => { input.down = true; toWorld(e); });
  window.addEventListener('pointerup', () => { input.down = false; input.px = input.py = null; });
  canvas.addEventListener('pointermove', toWorld);
  window.addEventListener('keydown', (e) => {
    const k = e.key.toLowerCase();
    if (k in KEYS) input.material = KEYS[k];
    if (k === '[') input.radius = Math.max(1, input.radius - 1);
    if (k === ']') input.radius = Math.min(24, input.radius + 1);
    if (k === 'd') input.debug = !input.debug;
    if (k === 'n') input.regen = true;
  });
  return input;
}

export function applyInput(input, world) {
  if (input.regen) {
    input.regen = false;
    world.generate((Math.random() * 0xFFFFFFFF) >>> 0);
  }
  if (!input.down) return;
  // interpolate from the previous point so fast drags leave no gaps
  const px = input.px ?? input.x, py = input.py ?? input.y;
  const steps = Math.max(1, Math.max(Math.abs(input.x - px), Math.abs(input.y - py)));
  for (let i = 0; i <= steps; i++) {
    const x = Math.round(px + (input.x - px) * (i / steps));
    const y = Math.round(py + (input.y - py) * (i / steps));
    world.paint(x, y, input.radius, input.material);
  }
  input.px = input.x;
  input.py = input.y;
}
