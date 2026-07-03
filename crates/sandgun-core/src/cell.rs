#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Material {
    Empty = 0,
    Rock = 1,
    Sand = 2,
    Water = 3,
    Oil = 4,
}

impl Material {
    pub fn from_u8(v: u8) -> Material {
        match v {
            1 => Material::Rock,
            2 => Material::Sand,
            3 => Material::Water,
            4 => Material::Oil,
            _ => Material::Empty,
        }
    }
    pub fn is_liquid(self) -> bool {
        matches!(self, Material::Water | Material::Oil)
    }
    pub fn is_solid(self) -> bool {
        matches!(self, Material::Rock)
    }
    pub fn is_powder(self) -> bool {
        matches!(self, Material::Sand)
    }
    /// Relative density. Only meaningful for Empty and liquids; solids/powders are 255.
    pub fn density(self) -> u8 {
        match self {
            Material::Empty => 0,
            Material::Oil => 1,
            Material::Water => 2,
            _ => 255,
        }
    }
    pub fn base_color(self) -> [u8; 3] {
        match self {
            Material::Empty => [26, 24, 32],
            Material::Rock => [110, 106, 100],
            Material::Sand => [216, 184, 108],
            Material::Water => [64, 120, 220],
            Material::Oil => [96, 78, 60],
        }
    }
}

// flags bit 7: reserved for rigid-body ownership (M2). Do not touch in M0.

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Cell {
    pub material: u8,
    pub shade: u8,
    pub flags: u8,
    pub aux: u8,
}
