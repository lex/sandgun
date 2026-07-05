use crate::cell::Material;

pub const P_FIRE_LIFETIME: usize = 0;
pub const P_SMOKE_LIFETIME: usize = 1;
pub const P_SMOKE_EMIT: usize = 2; // 0..1 chance per burning tick
pub const P_FIRE_FLICKER: usize = 3; // 0..1 chance a flame drifts upward
pub const P_FLAM_OIL: usize = 4; // 0..1 ignite chance per contact tick
pub const P_FLAM_MYCELIUM: usize = 5;
pub const P_FLAM_FLESH: usize = 6;
pub const P_FLAM_SPOREGAS: usize = 7;
pub const P_FUEL_OIL: usize = 8; // fuel ticks once ignited
pub const P_FUEL_MYCELIUM: usize = 9;
pub const P_FUEL_FLESH: usize = 10;
pub const P_FUEL_SPOREGAS: usize = 11;
pub const P_ACID_ETCH: usize = 12; // 0..1 dissolve chance per tick
pub const P_ACID_ETCH_ROCK: usize = 13;
pub const P_KINETIC_RADIUS: usize = 14;
pub const P_KINETIC_EJECTA: usize = 15; // 0..1 fraction of carved solids that fly
pub const P_INCENDIARY_RADIUS: usize = 16;
pub const P_ACID_BLOB_RADIUS: usize = 17;
pub const P_SPORE_BLOB_RADIUS: usize = 18;
// --- Parametric mushroom shape/decay (kept from the old M1c growth model; fruiting is now
// triggered by the M1e colony economy below, not these) ---
// (P_GROWTH_INTERVAL and P_GROWTH_BUDGET -- the old growth cadence/budget -- were removed in the
// M1e task 6 review: fully unused once the dormant grow() call site was deleted.)
pub const P_MAX_MUSHROOMS: usize = 19;   // global cap on simultaneous mushrooms (growing + decaying); see fruit_fed_colonies
pub const P_MUSH_HEIGHT_MIN: usize = 20;
pub const P_MUSH_HEIGHT_MAX: usize = 21;
pub const P_MUSH_CAP_MIN: usize = 22;
pub const P_MUSH_CAP_MAX: usize = 23;
pub const P_MUSH_REVEAL: usize = 24;     // cells revealed per growth tick per mushroom
pub const P_GUNFIRE_SPORE_CHANCE: usize = 25; // 0..1 a carved flesh cell releases spore gas
pub const P_ASH_CHANCE: usize = 26; // 0..1 chance burnt-out Mycelium/MushroomFlesh leaves Ash (else Empty)
// --- M1e living mycelium (the only growth model) ---
pub const P_MY_GROWTH_INTERVAL: usize = 27; // frames between mycelium growth ticks
pub const P_MY_TIP_CAP: usize = 28;         // max live tips per colony
pub const P_MY_EAT: usize = 29;             // richness->pool multiplier when a tip eats soil
pub const P_MY_FRUIT_THRESHOLD: usize = 30; // nutrient pool needed to fruit
pub const P_MY_FRUIT_COST: usize = 31;      // pool spent per fruiting event
pub const P_MY_DIEBACK: usize = 32;         // dieback rate
pub const P_MY_BRANCH_CHANCE: usize = 33;   // 0..1 periodic branch chance per tip
pub const P_MY_WORLDGEN_COLONIES: usize = 34; // number of colony origins seeded at worldgen
pub const P_SOIL_RICHNESS_MIN: usize = 35;  // worldgen baseline soil richness (aux) lower bound
pub const P_SOIL_RICHNESS_MAX: usize = 36;  // worldgen baseline soil richness (aux) upper bound
pub const P_MUSH_LIFESPAN: usize = 37;      // growth ticks a completed mushroom lives before decaying
// --- M1e playtest fixes: keep strands on substrate, wiggly, 2-wide ---
pub const P_MY_MAX_AIR_REACH: usize = 38;   // max consecutive Empty cells a tip may cross before it must reach Soil again
pub const P_MY_STRAND_WIDTH: usize = 39;    // cells wide a growing strand lays down (bounded 2-3)
pub const P_COUNT: usize = 40;

/// Hot-tunable sim parameters. Index constants are mirrored in web/src/params.js — keep in sync.
pub struct Params {
    pub values: [f32; P_COUNT],
}

impl Default for Params {
    fn default() -> Params {
        let mut v = [0.0; P_COUNT];
        v[P_FIRE_LIFETIME] = 40.0;
        v[P_SMOKE_LIFETIME] = 120.0;
        v[P_SMOKE_EMIT] = 0.20;
        v[P_FIRE_FLICKER] = 0.35;
        v[P_FLAM_OIL] = 0.65;
        v[P_FLAM_MYCELIUM] = 0.22;
        v[P_FLAM_FLESH] = 0.06;
        v[P_FLAM_SPOREGAS] = 1.0;
        v[P_FUEL_OIL] = 90.0;
        v[P_FUEL_MYCELIUM] = 130.0;
        v[P_FUEL_FLESH] = 220.0;
        v[P_FUEL_SPOREGAS] = 6.0;
        v[P_ACID_ETCH] = 0.35;
        v[P_ACID_ETCH_ROCK] = 0.04;
        v[P_KINETIC_RADIUS] = 5.0;
        v[P_KINETIC_EJECTA] = 0.35;
        v[P_INCENDIARY_RADIUS] = 3.0;
        v[P_ACID_BLOB_RADIUS] = 3.0;
        v[P_SPORE_BLOB_RADIUS] = 4.0;
        v[P_MAX_MUSHROOMS] = 6.0;
        v[P_MUSH_HEIGHT_MIN] = 6.0;
        v[P_MUSH_HEIGHT_MAX] = 16.0;
        v[P_MUSH_CAP_MIN] = 3.0;
        v[P_MUSH_CAP_MAX] = 7.0;
        v[P_MUSH_REVEAL] = 2.0;
        v[P_GUNFIRE_SPORE_CHANCE] = 0.5;
        v[P_ASH_CHANCE] = 0.25;
        v[P_MY_GROWTH_INTERVAL] = 8.0; // M1e playtest: 3 grew far too fast; slower cadence to read as organic growth
        v[P_MY_TIP_CAP] = 12.0;
        v[P_MY_EAT] = 1.0;
        v[P_MY_FRUIT_THRESHOLD] = 400.0;
        v[P_MY_FRUIT_COST] = 350.0;
        v[P_MY_DIEBACK] = 1.0;
        v[P_MY_BRANCH_CHANCE] = 0.04;
        v[P_MY_WORLDGEN_COLONIES] = 6.0;
        v[P_SOIL_RICHNESS_MIN] = 40.0;
        v[P_SOIL_RICHNESS_MAX] = 120.0;
        v[P_MUSH_LIFESPAN] = 900.0;
        v[P_MY_MAX_AIR_REACH] = 3.0;
        v[P_MY_STRAND_WIDTH] = 2.0;
        Params { values: v }
    }
}

impl Params {
    pub fn flammability(&self, m: Material) -> f32 {
        match m {
            Material::Oil => self.values[P_FLAM_OIL],
            Material::Mycelium => self.values[P_FLAM_MYCELIUM],
            Material::MushroomFlesh => self.values[P_FLAM_FLESH],
            Material::SporeGas => self.values[P_FLAM_SPOREGAS],
            _ => 0.0,
        }
    }
    pub fn fuel(&self, m: Material) -> u8 {
        (match m {
            Material::Oil => self.values[P_FUEL_OIL],
            Material::Mycelium => self.values[P_FUEL_MYCELIUM],
            Material::MushroomFlesh => self.values[P_FUEL_FLESH],
            Material::SporeGas => self.values[P_FUEL_SPOREGAS],
            _ => 0.0,
        })
        .clamp(0.0, 255.0) as u8
    }
}
