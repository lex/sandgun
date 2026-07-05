// Index map mirrors crates/sandgun-core/src/params.rs — keep in sync.
const INDEX = {
  fire_lifetime: 0, smoke_lifetime: 1, smoke_emit: 2, fire_flicker: 3,
  flam_oil: 4, flam_mycelium: 5, flam_flesh: 6, flam_sporegas: 7,
  fuel_oil: 8, fuel_mycelium: 9, fuel_flesh: 10, fuel_sporegas: 11,
  acid_etch: 12, acid_etch_rock: 13,
  kinetic_radius: 14, kinetic_ejecta: 15, incendiary_radius: 16,
  acid_blob_radius: 17, spore_blob_radius: 18,
  growth_interval: 19, growth_budget: 20, max_frontier: 21, max_reach: 22,
  water_accel: 23, maturity: 24, max_mushrooms: 25, fruit_chance: 26,
  mush_height_min: 27, mush_height_max: 28, mush_cap_min: 29, mush_cap_max: 30,
  mush_reveal: 31, puff_interval: 32, puff_spores: 33, reseed_chance: 34,
  gunfire_spore_chance: 35, ash_chance: 36,
  my_growth_interval: 37, my_tip_cap: 38, my_eat: 39, my_fruit_threshold: 40,
  my_fruit_cost: 41, my_dieback: 42, my_branch_chance: 43, my_worldgen_colonies: 44,
  soil_richness_min: 45, soil_richness_max: 46,
};

export async function loadParams(world) {
  try {
    const res = await fetch(`/params.json?t=${Date.now()}`); // bust cache on reload
    const json = await res.json();
    let applied = 0;
    for (const [name, value] of Object.entries(json)) {
      if (name in INDEX) {
        world.set_param(INDEX[name], value);
        applied++;
      } else {
        console.warn(`params.json: unknown param "${name}"`);
      }
    }
    console.log(`params: applied ${applied} values`);
  } catch (err) {
    console.warn('params: reload failed, keeping current values', err);
  }
}
