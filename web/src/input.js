import { M } from './scene.js';

const KEYS = {
  '1': M.SAND, '2': M.WATER, '3': M.OIL, '4': M.ROCK, '5': M.SOIL,
  '6': M.MYCELIUM, '7': M.FLESH, '8': M.SPOREGAS, '9': M.ACID,
  'f': M.FIRE, '0': M.EMPTY, 'e': M.EMPTY,
};
const NAMES = ['erase', 'rock', 'sand', 'water', 'oil', 'soil', 'mycelium',
  'flesh', 'spores', 'smoke', 'ash', 'acid', 'FIRE'];

export function attachInput(canvas, worldW, worldH) {
  const input = {
    down: false, x: 0, y: 0, px: null, py: null,
    material: M.SAND, radius: 4, debug: false, regen: false, reloadParams: false,
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
    if (k === 'p') input.reloadParams = true;
  });
  return input;
}

export function applyInput(input, world) {
  if (input.regen) {
    input.regen = false;
    world.generate((Math.random() * 0xFFFFFFFF) >>> 0);
  }
  if (input.reloadParams) {
    input.reloadParams = false;
    input.onReloadParams?.();
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
