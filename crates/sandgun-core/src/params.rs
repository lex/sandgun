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
pub const P_COUNT: usize = 19;

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
