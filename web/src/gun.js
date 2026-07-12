const AMMO = { z: 0, x: 1, c: 2, v: 3 }; // kinetic, incendiary, acid, spore
const AMMO_NAMES = ['kinetic', 'incendiary', 'acid', 'spore'];
const GUN_SPEED = 10;
const FIRE_COOLDOWN = 6; // frames

export function attachGun(canvas, worldW, worldH, cam) {
  const gun = {
    ammo: 0, aimX: worldW / 2, aimY: 0, firing: false, cooldown: 0,
    get status() { return `gun: ${AMMO_NAMES[this.ammo]}`; },
  };
  // cam.x/cam.y are the world-cell top-left of the visible window; adding them maps the
  // cursor to the correct world cell regardless of camera scroll.
  const toWorld = (e) => {
    const r = canvas.getBoundingClientRect();
    gun.aimX = (e.clientX - r.left) / r.width * worldW + cam.x;
    gun.aimY = (e.clientY - r.top) / r.height * worldH + cam.y;
  };
  canvas.addEventListener('mousemove', toWorld);
  canvas.addEventListener('mousedown', (e) => { if (e.button === 0) { gun.firing = true; toWorld(e); } });
  window.addEventListener('mouseup', (e) => { if (e.button === 0) gun.firing = false; });
  window.addEventListener('keydown', (e) => {
    const k = e.key.toLowerCase();
    if (k in AMMO) gun.ammo = AMMO[k];
  });
  return gun;
}

export function applyGun(gun, world) {
  if (gun.cooldown > 0) gun.cooldown--;
  if (!gun.firing || gun.cooldown > 0) return;
  const c = world.avatar_center();
  if (!c) return; // no avatar, no muzzle
  const [mx, my] = c;
  const dx = gun.aimX - mx;
  const dy = gun.aimY - my;
  const len = Math.hypot(dx, dy) || 1;
  world.fire(mx, my, (dx / len) * GUN_SPEED, (dy / len) * GUN_SPEED, gun.ammo);
  gun.cooldown = FIRE_COOLDOWN;
}
