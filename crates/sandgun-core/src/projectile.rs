#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ammo {
    Kinetic = 0,
    Incendiary = 1,
    Acid = 2,
    Spore = 3,
}

impl Ammo {
    pub fn from_u8(v: u8) -> Ammo {
        match v {
            1 => Ammo::Incendiary,
            2 => Ammo::Acid,
            3 => Ammo::Spore,
            _ => Ammo::Kinetic,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Projectile {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub ammo: Ammo,
    pub alive: bool,
}
