#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Material {
    Empty = 0,
    Rock = 1,
    Sand = 2,
    Water = 3,
    Oil = 4,
    Soil = 5,
    Mycelium = 6,
    MushroomFlesh = 7,
    SporeGas = 8,
    Smoke = 9,
    Ash = 10,
    Acid = 11,
    Fire = 12,
}

impl Material {
    pub fn from_u8(v: u8) -> Material {
        match v {
            1 => Material::Rock,
            2 => Material::Sand,
            3 => Material::Water,
            4 => Material::Oil,
            5 => Material::Soil,
            6 => Material::Mycelium,
            7 => Material::MushroomFlesh,
            8 => Material::SporeGas,
            9 => Material::Smoke,
            10 => Material::Ash,
            11 => Material::Acid,
            12 => Material::Fire,
            _ => Material::Empty,
        }
    }
    pub fn is_liquid(self) -> bool {
        matches!(self, Material::Water | Material::Oil | Material::Acid)
    }
    pub fn is_powder(self) -> bool {
        matches!(self, Material::Sand | Material::Soil | Material::Ash)
    }
    pub fn is_gas(self) -> bool {
        matches!(self, Material::SporeGas | Material::Smoke)
    }
    /// Static solids: never move (they can still burn).
    pub fn is_solid(self) -> bool {
        matches!(self, Material::Rock | Material::Mycelium | Material::MushroomFlesh)
    }
    /// Relative density among liquids (and Empty).
    pub fn density(self) -> u8 {
        match self {
            Material::Empty => 0,
            Material::Oil => 1,
            Material::Water => 2,
            Material::Acid => 3,
            _ => 255,
        }
    }
    /// Initial `aux` for a freshly created cell of this material.
    pub fn initial_aux(self) -> u8 {
        match self {
            Material::Fire => 40,   // flame lifetime in ticks
            Material::Smoke => 120, // fade time
            Material::Acid => 10,   // dissolve charges
            _ => 0,
        }
    }
    pub fn base_color(self) -> [u8; 3] {
        match self {
            Material::Empty => [26, 24, 32],
            Material::Rock => [110, 106, 100],
            Material::Sand => [216, 184, 108],
            Material::Water => [64, 120, 220],
            Material::Oil => [96, 78, 60],
            Material::Soil => [122, 86, 56],
            Material::Mycelium => [176, 168, 220],
            Material::MushroomFlesh => [232, 208, 186],
            Material::SporeGas => [154, 188, 96],
            Material::Smoke => [70, 70, 78],
            Material::Ash => [148, 142, 138],
            Material::Acid => [140, 224, 60],
            Material::Fire => [255, 150, 40],
        }
    }
}

pub const FLAG_PARITY: u8 = 0b0000_0001; // unused; kept for layout stability
pub const FLAG_BURNING: u8 = 0b0000_0010;
// flags bit 7: reserved for rigid-body ownership (M2). Do not touch.

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Cell {
    pub material: u8,
    pub shade: u8,
    pub flags: u8,
    pub aux: u8,
}

impl Cell {
    pub fn new(material: Material, shade: u8) -> Cell {
        Cell {
            material: material as u8,
            shade: shade & 3,
            flags: 0,
            aux: material.initial_aux(),
        }
    }
}
