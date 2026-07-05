use sandgun_core::world::World;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmWorld {
    inner: World,
}

#[wasm_bindgen]
impl WasmWorld {
    #[wasm_bindgen(constructor)]
    pub fn new(width: u32, height: u32) -> WasmWorld {
        WasmWorld { inner: World::new(width as usize, height as usize) }
    }
    pub fn step(&mut self) {
        self.inner.step();
    }
    pub fn generate(&mut self, seed: u32) {
        sandgun_core::worldgen::generate(&mut self.inner, seed);
    }
    pub fn paint(&mut self, x: i32, y: i32, radius: i32, material: u8) {
        self.inner.paint(x, y, radius, material);
    }
    pub fn fire(&mut self, x: f32, y: f32, vx: f32, vy: f32, ammo: u8) {
        self.inner.fire(x, y, vx, vy, ammo);
    }
    pub fn spawn_avatar(&mut self, x: f32, y: f32) {
        self.inner.spawn_avatar(x, y);
    }
    pub fn set_avatar_input(&mut self, left: bool, right: bool, jump: bool) {
        self.inner.set_avatar_input(left, right, jump);
    }
    pub fn avatar_xywh(&self) -> Option<Vec<f32>> {
        self.inner.avatar_xywh().map(|a| a.to_vec())
    }
    pub fn avatar_center(&self) -> Option<Vec<f32>> {
        self.inner.avatar_center().map(|a| a.to_vec())
    }
    pub fn projectile_count(&self) -> usize {
        self.inner.projectile_count()
    }
    pub fn particle_count(&self) -> usize {
        self.inner.particle_count()
    }
    pub fn render(&mut self) {
        self.inner.render_rgba();
    }
    pub fn rgba_ptr(&self) -> *const u8 {
        self.inner.rgba_ptr()
    }
    pub fn rgba_len(&self) -> usize {
        self.inner.width * self.inner.height * 4
    }
    pub fn active_ptr(&self) -> *const u8 {
        self.inner.active_ptr()
    }
    pub fn active_len(&self) -> usize {
        self.inner.active_len()
    }
    pub fn chunks_x(&self) -> usize {
        self.inner.chunks_x()
    }
    pub fn chunks_y(&self) -> usize {
        self.inner.chunks_y()
    }
    pub fn cells_processed(&self) -> u32 {
        self.inner.cells_processed as u32
    }
    pub fn set_param(&mut self, index: u32, value: f32) {
        self.inner.set_param(index, value);
    }
    pub fn mushroom_count(&self) -> usize {
        self.inner.mushroom_len()
    }
    pub fn colony_count(&self) -> usize {
        self.inner.colony_count()
    }
    pub fn tip_count(&self) -> usize {
        self.inner.tip_count()
    }
}
