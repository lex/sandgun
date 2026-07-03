// Material ids must match sandgun-core cell.rs
export const M = { EMPTY: 0, ROCK: 1, SAND: 2, WATER: 3, OIL: 4 };

// A basin, a spilling sand pile, and layered liquids — motion on first load.
export function seedScene(world, w, h) {
  for (let x = 80; x <= 300; x++) world.paint(x, 300, 1, M.ROCK); // basin floor
  for (let y = 240; y <= 300; y++) {
    world.paint(80, y, 1, M.ROCK);
    world.paint(300, y, 1, M.ROCK);
  }
  world.paint(190, 120, 14, M.SAND);   // sand blob, falls into basin
  world.paint(140, 60, 12, M.WATER);   // water blob
  world.paint(240, 40, 10, M.OIL);     // oil blob, ends up floating
  for (let x = 380; x <= 560; x += 4) world.paint(x, 200 + ((x / 4) % 3), 2, M.SAND); // loose ridge
}
