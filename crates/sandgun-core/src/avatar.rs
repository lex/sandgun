#[derive(Clone, Copy)]
pub struct Avatar {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub w: i32,
    pub h: i32,
    pub on_ground: bool,
    pub want_left: bool,
    pub want_right: bool,
    pub want_jump: bool,
}
