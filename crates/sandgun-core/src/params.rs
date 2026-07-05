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
// --- M1c growth ---
pub const P_GROWTH_INTERVAL: usize = 19; // frames between growth ticks
pub const P_GROWTH_BUDGET: usize = 20;   // frontier cells processed per growth tick
pub const P_MAX_FRONTIER: usize = 21;    // hard cap on frontier size
pub const P_MAX_REACH: usize = 22;       // max cells mycelium bridges into empty from soil
pub const P_WATER_ACCEL: usize = 23;     // extra colonize attempts when water-adjacent
pub const P_MATURITY: usize = 24;        // aux age a mycelium cell needs to be fruit-eligible
pub const P_MAX_MUSHROOMS: usize = 25;   // max simultaneous growing mushrooms
pub const P_FRUIT_CHANCE: usize = 26;    // 0..1 per growth tick a mature cell fruits
pub const P_MUSH_HEIGHT_MIN: usize = 27;
pub const P_MUSH_HEIGHT_MAX: usize = 28;
pub const P_MUSH_CAP_MIN: usize = 29;
pub const P_MUSH_CAP_MAX: usize = 30;
pub const P_MUSH_REVEAL: usize = 31;     // cells revealed per growth tick per mushroom
pub const P_PUFF_INTERVAL: usize = 32;   // growth ticks between cap spore puffs
pub const P_PUFF_SPORES: usize = 33;     // spore cells per puff
pub const P_RESEED_CHANCE: usize = 34;   // 0..1 a spore adjacent to soil seeds a colony
pub const P_GUNFIRE_SPORE_CHANCE: usize = 35; // 0..1 a carved flesh cell releases spore gas
pub const P_ASH_CHANCE: usize = 36; // 0..1 chance burnt-out Mycelium/MushroomFlesh leaves Ash (else Empty)
// --- M1e living mycelium (new organism model; old frontier params above stay until Task 6) ---
pub const P_MY_GROWTH_INTERVAL: usize = 37; // frames between mycelium growth ticks
pub const P_MY_TIP_CAP: usize = 38;         // max live tips per colony
pub const P_MY_EAT: usize = 39;             // richness->pool multiplier when a tip eats soil
pub const P_MY_FRUIT_THRESHOLD: usize = 40; // nutrient pool needed to fruit
pub const P_MY_FRUIT_COST: usize = 41;      // pool spent per fruiting event
pub const P_MY_DIEBACK: usize = 42;         // dieback rate
pub const P_MY_BRANCH_CHANCE: usize = 43;   // 0..1 periodic branch chance per tip
pub const P_MY_WORLDGEN_COLONIES: usize = 44; // number of colony origins seeded at worldgen
pub const P_SOIL_RICHNESS_MIN: usize = 45;  // worldgen baseline soil richness (aux) lower bound
pub const P_SOIL_RICHNESS_MAX: usize = 46;  // worldgen baseline soil richness (aux) upper bound
pub const P_MUSH_LIFESPAN: usize = 47;      // growth ticks a completed mushroom lives before decaying
pub const P_COUNT: usize = 48;

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
        v[P_GROWTH_INTERVAL] = 3.0;
        v[P_GROWTH_BUDGET] = 24.0;
        v[P_MAX_FRONTIER] = 4096.0;
        v[P_MAX_REACH] = 4.0;
        v[P_WATER_ACCEL] = 2.0;
        v[P_MATURITY] = 90.0;
        v[P_MAX_MUSHROOMS] = 6.0;
        v[P_FRUIT_CHANCE] = 0.02;
        v[P_MUSH_HEIGHT_MIN] = 6.0;
        v[P_MUSH_HEIGHT_MAX] = 16.0;
        v[P_MUSH_CAP_MIN] = 3.0;
        v[P_MUSH_CAP_MAX] = 7.0;
        v[P_MUSH_REVEAL] = 2.0;
        v[P_PUFF_INTERVAL] = 120.0;
        v[P_PUFF_SPORES] = 5.0;
        v[P_RESEED_CHANCE] = 0.10;
        v[P_GUNFIRE_SPORE_CHANCE] = 0.5;
        v[P_ASH_CHANCE] = 0.25;
        v[P_MY_GROWTH_INTERVAL] = 3.0;
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
