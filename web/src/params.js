// Index map mirrors crates/sandgun-core/src/params.rs — keep in sync.
const INDEX = {
  fire_lifetime: 0, smoke_lifetime: 1, smoke_emit: 2, fire_flicker: 3,
  flam_oil: 4, flam_mycelium: 5, flam_flesh: 6, flam_sporegas: 7,
  fuel_oil: 8, fuel_mycelium: 9, fuel_flesh: 10, fuel_sporegas: 11,
  acid_etch: 12, acid_etch_rock: 13,
  kinetic_radius: 14, kinetic_ejecta: 15, incendiary_radius: 16,
  acid_blob_radius: 17, spore_blob_radius: 18,
  max_mushrooms: 19,
  mush_height_min: 20, mush_height_max: 21, mush_cap_min: 22, mush_cap_max: 23,
  mush_reveal: 24, gunfire_spore_chance: 25, ash_chance: 26,
  my_growth_interval: 27, my_tip_cap: 28, my_eat: 29, my_fruit_threshold: 30,
  my_fruit_cost: 31, my_dieback: 32, my_branch_chance: 33, my_worldgen_colonies: 34,
  soil_richness_min: 35, soil_richness_max: 36, mush_lifespan: 37,
  my_max_air_reach: 38, my_strand_width: 39,
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
