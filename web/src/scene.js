// Material ids must match sandgun-core cell.rs
export const M = {
  EMPTY: 0, ROCK: 1, SAND: 2, WATER: 3, OIL: 4,
  SOIL: 5, MYCELIUM: 6, FLESH: 7, SPOREGAS: 8,
  SMOKE: 9, ASH: 10, ACID: 11, FIRE: 12,
};

// Mirrors Material::base_color() in crates/sandgun-core/src/cell.rs, indexed by material id --
// used by the entity overlay (main.js) to color in-flight particles (M1d task 3: particles are
// no longer stamped into the persistent world texture, so their color has to live in JS too).
export const MATERIAL_COLOR = [
  '#1a1820', // 0 EMPTY
  '#6e6a64', // 1 ROCK
  '#d8b86c', // 2 SAND
  '#4078dc', // 3 WATER
  '#604e3c', // 4 OIL
  '#7a5638', // 5 SOIL
  '#b0a8dc', // 6 MYCELIUM
  '#e8d0ba', // 7 FLESH
  '#9abc60', // 8 SPOREGAS
  '#46464e', // 9 SMOKE
  '#948e8a', // 10 ASH
  '#8ce03c', // 11 ACID
  '#ff9628', // 12 FIRE
];

// Projectiles/avatar aren't grid materials -- these match the colors render_rgba used to stamp
// before entities moved to this overlay pass.
export const PROJECTILE_COLOR = '#fff0c8'; // hot near-white tracer
export const AVATAR_COLOR = '#5adcf0';     // cyan
