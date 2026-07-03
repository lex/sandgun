# SANDGUN M1a — "The World Burns" Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The static fungal world, flammable: full material roster (soil, mycelium, mushroom flesh, spore gas, smoke, ash, acid, fire), cell-state fire with fuses and detonating spore pockets, fungal worldgen, and hot-reloadable tuning — all driveable with the debug brush. Guns/particles/camera are the next plan (M1b).

**Architecture:** Extends `sandgun-core` in place: `Material` grows to 13 ids; a `Params` module holds hot-tunable floats; `update_cell` dispatches to new behaviors (burning, fire, gas, acid); worldgen gains the fungal biome. Spec: `docs/superpowers/specs/2026-07-03-mushroom-vision-design.md`.

**Tech Stack:** unchanged (Rust stable, wasm-bindgen/wasm-pack, Vite, WebGL2).

## Global Constraints

- Cell stays exactly 4 bytes; flags **bit 7 reserved** (M2 rigid bodies); flags **bit 1 = FLAG_BURNING** (new); bit 0 remains the unused `FLAG_PARITY` for layout stability.
- NO per-cell temperature. Fire is cell state: `FLAG_BURNING` on fuel materials + a free-flame `Fire` material; fuel/lifetimes count down in `aux`.
- Chunk sleeping is sacred: every new behavior must reach a rest state (finite fuel, finite smoke lifetime, gas rest-seeking, acid charges). `settled` tests are the enforcement.
- Material ids (u8, contact tables key off these): `Empty=0, Rock=1, Sand=2, Water=3, Oil=4, Soil=5, Mycelium=6, MushroomFlesh=7, SporeGas=8, Smoke=9, Ash=10, Acid=11, Fire=12`.
- Out-of-bounds reads via `material_at` return `Rock` (world border blocks everything).
- Sim logic only in `sandgun-core`; wasm crate stays glue-only.
- All commits end with: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` (second `-m`). Run cargo from repo root; `export PATH="$HOME/.cargo/bin:$PATH"` if cargo is missing.
- Work on a branch: `git checkout -b m1a-world-burns` before Task 1.

---

### Task 1: Material roster, params module, Cell::new

**Files:**
- Modify: `crates/sandgun-core/src/cell.rs` (replace whole file)
- Create: `crates/sandgun-core/src/params.rs`
- Modify: `crates/sandgun-core/src/lib.rs` (add `pub mod params;`)
- Modify: `crates/sandgun-core/src/world.rs` (add `params` field, `chance`, `material_at`; `paint` uses `Cell::new`)
- Create: `crates/sandgun-core/tests/materials.rs`

**Interfaces:**
- Consumes: existing `World`.
- Produces: `Material` with 13 variants and class helpers (`is_liquid/is_powder/is_gas/is_solid`, `density`, `initial_aux`, `base_color`); `FLAG_BURNING = 0b0000_0010`; `Cell::new(material: Material, shade: u8)` (sets `initial_aux`); `Params` (`values: [f32; P_COUNT]`, `Default`, `flammability(m)`, `fuel(m)`) with public index constants; `World.params: Params` (pub field); `World::chance(&mut self, p: f32) -> bool`; `World::material_at(&self, x: isize, y: isize) -> Material` (OOB → Rock). Existing behavior unchanged — the full old suite must still pass.

- [ ] **Step 1: Write the failing tests**

`crates/sandgun-core/tests/materials.rs`:
```rust
use sandgun_core::cell::{Cell, Material, FLAG_BURNING};
use sandgun_core::params::{Params, P_FLAM_MYCELIUM};
use sandgun_core::world::World;

#[test]
fn material_ids_roundtrip() {
    for id in 0..=12u8 {
        assert_eq!(Material::from_u8(id) as u8, id, "id {id} must roundtrip");
    }
    assert_eq!(Material::from_u8(200), Material::Empty);
}

#[test]
fn material_classes_are_disjoint() {
    for id in 0..=12u8 {
        let m = Material::from_u8(id);
        let classes = [m.is_liquid(), m.is_powder(), m.is_gas(), m.is_solid()];
        assert!(classes.iter().filter(|&&c| c).count() <= 1, "{m:?} is in multiple classes");
    }
    assert!(Material::Soil.is_powder());
    assert!(Material::Ash.is_powder());
    assert!(Material::Acid.is_liquid());
    assert!(Material::SporeGas.is_gas());
    assert!(Material::Smoke.is_gas());
    assert!(Material::Mycelium.is_solid());
    assert!(Material::MushroomFlesh.is_solid());
}

#[test]
fn cell_new_sets_initial_aux() {
    assert_eq!(Cell::new(Material::Fire, 0).aux, 40);
    assert_eq!(Cell::new(Material::Acid, 0).aux, 10);
    assert_eq!(Cell::new(Material::Sand, 0).aux, 0);
    assert_eq!(Cell::new(Material::Fire, 0).flags & FLAG_BURNING, 0);
}

#[test]
fn params_default_and_lookup() {
    let p = Params::default();
    assert!(p.flammability(Material::SporeGas) >= 1.0);
    assert!(p.flammability(Material::Rock) == 0.0);
    assert!(p.fuel(Material::Mycelium) > 0);
    assert!(p.values[P_FLAM_MYCELIUM] > 0.0);
}

#[test]
fn painted_acid_gets_charges() {
    let mut w = World::new(64, 64);
    w.paint(10, 10, 0, Material::Acid as u8);
    // charges live in aux; verified indirectly: world exposes get() only, so assert via
    // a step not consuming it in isolation — the cell must still be acid after one step.
    w.step();
    let acid_somewhere = (0..64).any(|x| (0..64).any(|y| w.get(x, y) == Material::Acid));
    assert!(acid_somewhere, "fresh acid must not instantly vanish");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sandgun-core --test materials`
Expected: FAIL — new variants/module don't exist.

- [ ] **Step 3: Implement**

Replace `crates/sandgun-core/src/cell.rs` entirely:
```rust
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
```

Create `crates/sandgun-core/src/params.rs`:
```rust
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
pub const P_COUNT: usize = 14;

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
```

In `crates/sandgun-core/src/lib.rs` add `pub mod params;`.

In `crates/sandgun-core/src/world.rs`:
- Add to the `use` line: `use crate::params::Params;`
- Add field to `World`: `pub params: Params,` and initialize in `new()`: `params: Params::default(),`
- Add inside `impl World`:
```rust
/// true with probability p (0..1); deterministic via the sim RNG.
pub(crate) fn chance(&mut self, p: f32) -> bool {
    if p <= 0.0 {
        return false;
    }
    (self.next_rand() >> 8) as f32 / 16_777_216.0 < p
}

/// Material at (x, y); out of bounds reads as Rock (the border blocks everything).
pub(crate) fn material_at(&self, x: isize, y: isize) -> Material {
    if !self.in_bounds(x, y) {
        return Material::Rock;
    }
    Material::from_u8(self.cells[self.idx(x as usize, y as usize)].material)
}
```
- In `paint()`, replace the cell construction with:
```rust
let shade = (self.next_rand() & 3) as u8;
let i = self.idx(x as usize, y as usize);
self.cells[i] = Cell::new(Material::from_u8(material), shade);
```

- [ ] **Step 4: Run the FULL suite**

Run: `cargo test -p sandgun-core`
Expected: all pass (old 20 + 5 new). The old suite guards that the roster expansion changed nothing for existing materials.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: fungal material roster, params module, Cell::new (M1a task 1)"
```

---

### Task 2: Gas movement — smoke fades, spore gas pools and rests

**Files:**
- Modify: `crates/sandgun-core/src/world.rs` (dispatch + `update_gas`)
- Create: `crates/sandgun-core/tests/gases.rs`

**Interfaces:**
- Consumes: Task 1 roster, `swap_cells`, `chance`, `material_at`, rest-seeking pattern from liquids.
- Produces: gases rise (straight, then random diagonal), then rest-seek sideways toward a cell they could rise from; `Smoke` decays via `aux` and vanishes; `SporeGas` is persistent and rests (pools at cave ceilings — an upside-down liquid). New `update_cell` dispatch (given in full here, implemented once, reused by Tasks 3–4: burning check first, then Fire/Acid/gas/powder/liquid, static solids cost nothing).

- [ ] **Step 1: Write the failing tests**

`crates/sandgun-core/tests/gases.rs`:
```rust
use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn spore_gas_rises_and_pools_at_ceiling() {
    let mut w = World::new(64, 64);
    // sealed box: ceiling at y=20, walls x=20/x=30, floor y=30
    for x in 20..=30 {
        w.paint(x, 20, 0, Material::Rock as u8);
        w.paint(x, 30, 0, Material::Rock as u8);
    }
    for y in 20..=30 {
        w.paint(20, y, 0, Material::Rock as u8);
        w.paint(30, y, 0, Material::Rock as u8);
    }
    w.paint(25, 28, 1, Material::SporeGas as u8); // low in the box
    for _ in 0..300 {
        w.step();
    }
    assert_eq!(w.get(25, 28), Material::Empty, "gas must leave the floor area");
    let at_ceiling = (21..30).any(|x| w.get(x, 21) == Material::SporeGas);
    assert!(at_ceiling, "gas must pool under the ceiling");
    w.step();
    assert_eq!(w.cells_processed, 0, "pooled gas must come to rest");
}

#[test]
fn smoke_fades_away_and_world_settles() {
    let mut w = World::new(64, 64);
    w.paint(32, 40, 2, Material::Smoke as u8);
    for _ in 0..400 {
        w.step();
    }
    let any_smoke = (0..64).any(|x| (0..64).any(|y| w.get(x, y) == Material::Smoke));
    assert!(!any_smoke, "smoke must fully dissipate");
    w.step();
    assert_eq!(w.cells_processed, 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sandgun-core --test gases`
Expected: FAIL — gases currently have no behavior (dispatch ignores them).

- [ ] **Step 3: Implement dispatch + gas behavior**

In `crates/sandgun-core/src/world.rs`, REPLACE `update_cell` with (this is the final dispatch — Tasks 3–4 fill in the stubs):
```rust
fn update_cell(&mut self, x: usize, y: usize) {
    let i = self.idx(x, y);
    if self.stamp[i] == self.frame_u8() {
        return; // already moved this frame
    }
    let cell = self.cells[i];
    let mat = Material::from_u8(cell.material);
    if mat == Material::Empty {
        return;
    }
    if cell.flags & FLAG_BURNING != 0 {
        self.cells_processed += 1;
        self.update_burning(x, y);
        return;
    }
    match mat {
        Material::Fire => {
            self.cells_processed += 1;
            self.update_fire(x, y);
        }
        Material::Acid => {
            self.cells_processed += 1;
            self.update_acid(x, y);
        }
        m if m.is_gas() => {
            self.cells_processed += 1;
            self.update_gas(x, y, m);
        }
        m if m.is_powder() => {
            self.cells_processed += 1;
            self.update_powder(x, y);
        }
        m if m.is_liquid() => {
            self.cells_processed += 1;
            self.update_liquid(x, y, m);
        }
        _ => {} // static solids (Rock, Mycelium, MushroomFlesh) cost nothing
    }
}
```
Add `FLAG_BURNING` to the cell.rs import in world.rs (`use crate::cell::{Cell, Material, FLAG_BURNING};`).

Add stubs (filled by Tasks 3–4) and the real `update_gas` inside `impl World`:
```rust
fn update_burning(&mut self, _x: usize, _y: usize) {
    // Task 3
}

fn update_fire(&mut self, _x: usize, _y: usize) {
    // Task 3
}

fn update_acid(&mut self, _x: usize, _y: usize) {
    // Task 4
}

fn update_gas(&mut self, x: usize, y: usize, mat: Material) {
    let i = self.idx(x, y);
    if mat == Material::Smoke {
        if self.cells[i].aux == 0 {
            self.cells[i] = Cell::default();
            self.wake(x, y);
            return;
        }
        self.cells[i].aux -= 1;
        self.wake(x, y); // fading smoke keeps its chunk lit until it dies
    }
    let (xi, yi) = (x as isize, y as isize);
    // rise straight, then random diagonal, into empty
    let first_dx = if self.next_rand() & 1 == 0 { -1 } else { 1 };
    for (nx, ny) in [(xi, yi - 1), (xi + first_dx, yi - 1), (xi - first_dx, yi - 1)] {
        if self.in_bounds(nx, ny) && self.material_at(nx, ny) == Material::Empty {
            self.swap_cells(x, y, nx as usize, ny as usize);
            return;
        }
    }
    // rest-seeking sideways: slide only toward a cell it could rise from
    let first_dir: isize = if self.next_rand() & 1 == 0 { 1 } else { -1 };
    for dir in [first_dir, -first_dir] {
        let mut nx = xi;
        for _ in 0..DISPERSION {
            nx += dir;
            if !self.in_bounds(nx, yi) || self.material_at(nx, yi) != Material::Empty {
                break;
            }
            if self.material_at(nx, yi - 1) == Material::Empty {
                self.swap_cells(x, y, nx as usize, y);
                return;
            }
        }
    }
}
```

- [ ] **Step 4: Run the FULL suite**

Run: `cargo test -p sandgun-core`
Expected: all pass (27). If `settled_world_processes_zero_cells` regressed, the dispatch's static-solid arm is counting rocks — it must not.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: gas movement — smoke fades, spore gas pools at ceilings (M1a task 2)"
```

---

### Task 3: Fire — ignition, fuses, detonating spore gas, extinguishing

**Files:**
- Modify: `crates/sandgun-core/src/world.rs` (fill `update_burning`/`update_fire`, add helpers; render override)
- Create: `crates/sandgun-core/tests/fire.rs`

**Interfaces:**
- Consumes: Tasks 1–2 (params, dispatch, gases, `chance`, `material_at`).
- Produces: `ignite_neighbors(x, y)` (4-neighborhood, chance = `params.flammability`, sets `FLAG_BURNING` + `aux = params.fuel`); `emit_smoke_above(x, y)`; burning lifecycle (water-extinguish first, fuel countdown, burn products: Mycelium/MushroomFlesh → Ash, SporeGas → Fire, others → Empty; burning liquids keep flowing); free-flame `Fire` cells (lifetime, flicker upward, ignite, die to Empty); render shows burning cells as animated fire.

- [ ] **Step 1: Write the failing tests**

`crates/sandgun-core/tests/fire.rs`:
```rust
use sandgun_core::cell::Material;
use sandgun_core::world::World;

fn count(w: &World, m: Material) -> usize {
    let mut n = 0;
    for y in 0..w.height {
        for x in 0..w.width {
            if w.get(x, y) == m {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn mycelium_vein_burns_like_a_fuse() {
    let mut w = World::new(128, 64);
    for x in 30..=90 {
        w.paint(x, 40, 0, Material::Rock as u8); // shelf
        w.paint(x, 39, 0, Material::Mycelium as u8); // vein on the shelf
    }
    w.paint(29, 39, 0, Material::Fire as u8); // light the left end
    // keep relighting the tip for a few frames so the probabilistic catch is certain
    for _ in 0..8 {
        w.paint(29, 39, 0, Material::Fire as u8);
        w.step();
    }
    for _ in 0..3000 {
        w.step();
    }
    assert_eq!(count(&w, Material::Mycelium), 0, "the whole vein must burn through");
    assert!(count(&w, Material::Ash) > 20, "burnt mycelium leaves ash");
    for _ in 0..600 {
        w.step();
    }
    w.step();
    assert_eq!(w.cells_processed, 0, "world must settle after the burn");
}

#[test]
fn water_stops_a_fuse() {
    let mut w = World::new(128, 64);
    for x in 30..=90 {
        w.paint(x, 40, 0, Material::Rock as u8);
        w.paint(x, 39, 0, Material::Mycelium as u8);
    }
    // water block interrupting the vein, walled so it stays put
    w.paint(60, 38, 0, Material::Rock as u8);
    w.paint(62, 38, 0, Material::Rock as u8);
    w.paint(61, 39, 0, Material::Water as u8);
    w.paint(61, 38, 0, Material::Water as u8);
    for _ in 0..8 {
        w.paint(29, 39, 0, Material::Fire as u8);
        w.step();
    }
    for _ in 0..3000 {
        w.step();
    }
    let right_side: usize = (63..=90).filter(|&x| w.get(x, 39) == Material::Mycelium).count();
    assert!(right_side > 20, "vein beyond the waterline must survive");
}

#[test]
fn spore_gas_detonates_in_a_chain() {
    let mut w = World::new(64, 64);
    // sealed box full of spore gas
    for x in 20..=40 {
        w.paint(x, 20, 0, Material::Rock as u8);
        w.paint(x, 32, 0, Material::Rock as u8);
    }
    for y in 20..=32 {
        w.paint(20, y, 0, Material::Rock as u8);
        w.paint(40, y, 0, Material::Rock as u8);
    }
    for x in 21..40 {
        for y in 21..32 {
            w.paint(x, y, 0, Material::SporeGas as u8);
        }
    }
    let before = count(&w, Material::SporeGas);
    assert!(before > 150);
    w.paint(21, 31, 0, Material::Fire as u8); // one spark in the corner
    for _ in 0..240 {
        w.step();
    }
    assert_eq!(count(&w, Material::SporeGas), 0, "one spark must consume the whole pocket");
}

#[test]
fn lone_fire_burns_out_and_settles() {
    let mut w = World::new(64, 64);
    w.paint(32, 32, 1, Material::Fire as u8);
    for _ in 0..600 {
        w.step();
    }
    assert_eq!(count(&w, Material::Fire), 0);
    assert_eq!(count(&w, Material::Smoke), 0);
    w.step();
    assert_eq!(w.cells_processed, 0);
}

#[test]
fn flammability_zero_param_prevents_ignition() {
    let mut w = World::new(64, 64);
    w.params.values[sandgun_core::params::P_FLAM_MYCELIUM] = 0.0;
    for x in 20..=40 {
        w.paint(x, 40, 0, Material::Rock as u8);
        w.paint(x, 39, 0, Material::Mycelium as u8);
    }
    for _ in 0..8 {
        w.paint(19, 39, 0, Material::Fire as u8);
        w.step();
    }
    for _ in 0..1000 {
        w.step();
    }
    assert_eq!(count(&w, Material::Mycelium), 21, "param at 0 must make mycelium fireproof");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sandgun-core --test fire`
Expected: FAIL — fire stubs do nothing (mycelium survives, fire cells persist forever, etc.).

- [ ] **Step 3: Implement fire**

In `crates/sandgun-core/src/world.rs`, replace the two stubs and add helpers (uses `P_*` consts — extend the imports with `use crate::params::{Params, P_FIRE_FLICKER, P_SMOKE_EMIT, P_SMOKE_LIFETIME};`):
```rust
fn update_burning(&mut self, x: usize, y: usize) {
    let i = self.idx(x, y);
    let mat = Material::from_u8(self.cells[i].material);
    let (xi, yi) = (x as isize, y as isize);
    // water above or beside extinguishes; water BELOW does not, so an oil
    // slick floating on a pool keeps burning
    for (nx, ny) in [(xi, yi - 1), (xi + 1, yi), (xi - 1, yi)] {
        if self.material_at(nx, ny) == Material::Water {
            self.cells[i].flags &= !FLAG_BURNING;
            self.cells[i].aux = 0;
            self.wake(x, y);
            return;
        }
    }
    if self.cells[i].aux == 0 {
        // fuel spent: burn to the product material
        let product = match mat {
            Material::Mycelium | Material::MushroomFlesh => Material::Ash,
            Material::SporeGas => Material::Fire, // the detonation flash
            _ => Material::Empty,
        };
        let shade = (self.next_rand() & 3) as u8;
        self.cells[i] = Cell::new(product, shade);
        self.stamp[i] = self.frame_u8();
        self.wake(x, y);
        return;
    }
    self.cells[i].aux -= 1;
    self.wake(x, y); // burning cells stay hot until spent
    self.ignite_neighbors(x, y);
    self.emit_smoke_above(x, y);
    if mat.is_liquid() {
        self.update_liquid(x, y, mat); // burning oil keeps flowing
    }
}

fn update_fire(&mut self, x: usize, y: usize) {
    let i = self.idx(x, y);
    if self.cells[i].aux == 0 {
        self.cells[i] = Cell::default();
        self.wake(x, y);
        return;
    }
    self.cells[i].aux -= 1;
    self.wake(x, y);
    self.ignite_neighbors(x, y);
    self.emit_smoke_above(x, y);
    // flicker upward into empty space
    if self.chance(self.params.values[P_FIRE_FLICKER]) {
        let (xi, yi) = (x as isize, y as isize);
        if self.material_at(xi, yi - 1) == Material::Empty {
            self.swap_cells(x, y, x, y - 1);
        }
    }
}

fn ignite_neighbors(&mut self, x: usize, y: usize) {
    let (xi, yi) = (x as isize, y as isize);
    for (nx, ny) in [(xi, yi - 1), (xi + 1, yi), (xi, yi + 1), (xi - 1, yi)] {
        if !self.in_bounds(nx, ny) {
            continue;
        }
        let ni = self.idx(nx as usize, ny as usize);
        if self.cells[ni].flags & FLAG_BURNING != 0 {
            continue;
        }
        let nmat = Material::from_u8(self.cells[ni].material);
        let p = self.params.flammability(nmat);
        if p > 0.0 && self.chance(p) {
            self.cells[ni].flags |= FLAG_BURNING;
            self.cells[ni].aux = self.params.fuel(nmat);
            self.wake(nx as usize, ny as usize);
        }
    }
}

fn emit_smoke_above(&mut self, x: usize, y: usize) {
    if y == 0 {
        return;
    }
    let above = self.idx(x, y - 1);
    if Material::from_u8(self.cells[above].material) == Material::Empty
        && self.chance(self.params.values[P_SMOKE_EMIT])
    {
        let shade = (self.next_rand() & 3) as u8;
        let mut c = Cell::new(Material::Smoke, shade);
        c.aux = self.params.values[P_SMOKE_LIFETIME].clamp(0.0, 255.0) as u8;
        self.cells[above] = c;
        self.stamp[above] = self.frame_u8();
        self.wake(x, y - 1);
    }
}
```

In `render_rgba`, make burning cells render as animated flame — replace the color computation for each cell with:
```rust
let burning = cell.flags & FLAG_BURNING != 0;
let mat = Material::from_u8(cell.material);
let (base, j) = if burning || mat == Material::Fire {
    // animate between deep orange and yellow using the countdown for flicker
    let hot = (cell.aux & 7) as i16 * 10;
    ([255u8, (150 + hot.min(70)) as u8, 40u8], 0i16)
} else {
    let j = if mat == Material::Empty { 0 } else { (cell.shade & 3) as i16 * 6 - 9 };
    (mat.base_color(), j)
};
let o = i * 4;
self.rgba[o] = (base[0] as i16 + j).clamp(0, 255) as u8;
self.rgba[o + 1] = (base[1] as i16 + j).clamp(0, 255) as u8;
self.rgba[o + 2] = (base[2] as i16 + j).clamp(0, 255) as u8;
self.rgba[o + 3] = 255;
```

- [ ] **Step 4: Run the FULL suite**

Run: `cargo test -p sandgun-core`
Expected: all pass (32). The old render test still passes (sand/empty paths unchanged).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: fire — fuses, spore-gas detonation, extinguishing, flame render (M1a task 3)"
```

---

### Task 4: Acid — dissolves terrain, spends itself

**Files:**
- Modify: `crates/sandgun-core/src/world.rs` (fill `update_acid`)
- Create: `crates/sandgun-core/tests/acid.rs`

**Interfaces:**
- Consumes: dispatch stub from Task 2, `update_liquid`, params.
- Produces: acid tries one random 4-neighbor per tick: immune (Empty/Acid/Fire/gases/Water) never dissolve; Rock dissolves at `P_ACID_ETCH_ROCK`, everything else at `P_ACID_ETCH`; each dissolve costs one `aux` charge (spent acid vanishes); otherwise acid moves as a liquid (density 3 — sinks below water and oil).

- [ ] **Step 1: Write the failing tests**

`crates/sandgun-core/tests/acid.rs`:
```rust
use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn acid_eats_through_a_sand_plug() {
    // Amended: the original floating shelf fell by gravity, passing vacuously.
    // This plug rests on the world floor between rock walls — only acid can shrink it.
    let mut w = World::new(64, 64);
    for y in 56..64 {
        w.paint(28, y, 0, Material::Rock as u8);
        w.paint(36, y, 0, Material::Rock as u8);
    }
    for x in 29..36 {
        for y in 58..64 {
            w.paint(x, y, 0, Material::Sand as u8);
        }
    }
    let initial: usize = (29..36).map(|x| (58..64).filter(|&y| w.get(x, y) == Material::Sand).count()).sum();
    assert_eq!(initial, 42);
    w.paint(32, 56, 1, Material::Acid as u8);
    for _ in 0..1500 {
        w.step();
    }
    let sand: usize = (0..64).map(|x| (0..64).filter(|&y| w.get(x, y) == Material::Sand).count()).sum();
    assert!(sand < 35, "acid must corrode well into the plug ({sand}/42 cells left)");
}

#[test]
fn acid_spends_its_charges_and_vanishes() {
    // NOTE (amended after implementation feedback): acid that tunnels free and ends up
    // isolated correctly RESTS as an inert puddle with unspent charges — charges bound
    // activity, not existence. To test full spend, the acid must never run out of food:
    // a deep full-width sand bed.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        for y in 40..64 {
            w.paint(x, y, 0, Material::Sand as u8);
        }
    }
    w.paint(32, 38, 1, Material::Acid as u8);
    for _ in 0..3000 {
        w.step();
    }
    let acid_left = (0..64).any(|x| (0..64).any(|y| w.get(x, y) == Material::Acid));
    assert!(!acid_left, "every acid cell must spend its charges into the sand bed");
    for _ in 0..300 {
        w.step();
    }
    w.step();
    assert_eq!(w.cells_processed, 0, "world must settle after the acid is spent");
}

#[test]
fn acid_barely_touches_rock() {
    let mut w = World::new(64, 64);
    for x in 20..=40 {
        w.paint(x, 40, 0, Material::Rock as u8);
    }
    // walls so it can't slide off the shelf
    w.paint(29, 39, 0, Material::Rock as u8);
    w.paint(31, 39, 0, Material::Rock as u8);
    w.paint(30, 39, 0, Material::Acid as u8);
    for _ in 0..200 {
        w.step();
    }
    // one acid cell at rock etch-rate can nibble at most a couple of cells in 200 ticks
    let rocks: usize = (20..=40).filter(|&x| w.get(x, 40) == Material::Rock).count();
    assert!(rocks >= 19, "rock must mostly survive brief acid contact ({rocks}/21 left)");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sandgun-core --test acid`
Expected: `acid_eats_through_a_sand_shelf` and `acid_spends_its_charges_and_vanishes` FAIL (stub does nothing). `acid_barely_touches_rock` may pass vacuously — note it.

- [ ] **Step 3: Implement acid**

Replace the `update_acid` stub in `crates/sandgun-core/src/world.rs` (extend the params import with `P_ACID_ETCH, P_ACID_ETCH_ROCK`):
```rust
fn acid_etch_chance(&self, m: Material) -> f32 {
    match m {
        Material::Empty
        | Material::Acid
        | Material::Fire
        | Material::Smoke
        | Material::SporeGas
        | Material::Water => 0.0,
        Material::Rock => self.params.values[P_ACID_ETCH_ROCK],
        _ => self.params.values[P_ACID_ETCH],
    }
}

fn update_acid(&mut self, x: usize, y: usize) {
    let (xi, yi) = (x as isize, y as isize);
    let dirs = [(xi, yi + 1), (xi - 1, yi), (xi + 1, yi), (xi, yi - 1)];
    // stay awake while anything etchable is adjacent — otherwise a run of missed
    // rolls could let the chunk sleep mid-meal. Charges still bound total lifetime.
    if dirs
        .iter()
        .any(|&(nx, ny)| self.in_bounds(nx, ny) && self.acid_etch_chance(self.material_at(nx, ny)) > 0.0)
    {
        self.wake(x, y);
    }
    // try to dissolve one random 4-neighbor
    let (nx, ny) = dirs[(self.next_rand() % 4) as usize];
    if self.in_bounds(nx, ny) {
        let p = self.acid_etch_chance(self.material_at(nx, ny));
        if p > 0.0 && self.chance(p) {
            let ni = self.idx(nx as usize, ny as usize);
            self.cells[ni] = Cell::default();
            self.stamp[ni] = self.frame_u8();
            self.wake(nx as usize, ny as usize);
            let i = self.idx(x, y);
            if self.cells[i].aux <= 1 {
                self.cells[i] = Cell::default(); // spent
            } else {
                self.cells[i].aux -= 1;
            }
            self.wake(x, y);
            return;
        }
    }
    self.update_liquid(x, y, Material::Acid);
}
```

- [ ] **Step 4: Run the FULL suite**

Run: `cargo test -p sandgun-core`
Expected: all pass (35).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: acid dissolves terrain and spends itself (M1a task 4)"
```

---

### Task 5: Fungal worldgen — soil crust, mycelium veins, mushroom groves, spore pockets

**Files:**
- Modify: `crates/sandgun-core/src/worldgen.rs`
- Modify: `crates/sandgun-core/tests/worldgen.rs` (extend the presence test)

**Interfaces:**
- Consumes: existing `generate` (surface/rock/caves/pockets), `set`/`blob` helpers, `GenRng`.
- Produces: generation order becomes — surface → rock → caves → **soil crust** (6–10 cells beneath the surface where rock remains) → **mycelium veins** (random walks converting soil) → **mushroom groves** (stem + cap of MushroomFlesh grown up from cave floors, incl. 1–2 giants) → **spore pockets** (SporeGas blobs in cave air; they rise and pool on their own) → sand dunes → water/oil pockets → wake_all. Same seed → same world still holds.

- [ ] **Step 1: Extend the failing test**

In `crates/sandgun-core/tests/worldgen.rs`, extend `generated_world_has_terrain_air_and_materials` with these assertions (keep the existing ones):
```rust
    assert!(count(&w, Material::Soil) > 300, "soil crust present");
    assert!(count(&w, Material::Mycelium) > 60, "mycelium veins present");
    assert!(count(&w, Material::MushroomFlesh) > 80, "mushroom groves present");
    assert!(count(&w, Material::SporeGas) > 40, "spore pockets present");
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p sandgun-core --test worldgen`
Expected: the extended test FAILS (none of the fungal materials generate yet). `generated_world_fully_settles` still passes.

- [ ] **Step 3: Implement the fungal biome**

In `crates/sandgun-core/src/worldgen.rs`, insert between the cave-carving block and the sand-dune block:
```rust
    // 3b. soil crust: the top skin of remaining rock becomes colonizable soil
    for x in 0..w {
        let depth = 6 + (rng.next() % 5) as usize;
        for d in 0..depth {
            let yy = surface[x] as usize + d;
            if yy < h && world.get(x, yy) == Material::Rock {
                set(world, x, yy, Material::Soil, &mut rng);
            }
        }
    }

    // 3c. mycelium veins: random walks through the soil
    for _ in 0..(w / 12).max(6) {
        let mut vx = rng.range(2, w as i32 - 2);
        let mut vy = surface[vx as usize] + rng.range(1, 6);
        let len = rng.range(15, 50);
        for _ in 0..len {
            if vx < 1 || vy < 1 || vx as usize >= w - 1 || vy as usize >= h - 1 {
                break;
            }
            if world.get(vx as usize, vy as usize) == Material::Soil {
                set(world, vx as usize, vy as usize, Material::Mycelium, &mut rng);
            }
            // wander, biased sideways so veins read as veins
            vx += rng.range(-1, 2);
            vy += if rng.chance(1, 3) { rng.range(-1, 2) } else { 0 };
        }
    }

    // 3d. mushroom groves: stems + caps grown up from cave floors
    let mut floors: Vec<(usize, usize)> = Vec::new();
    for x in (2..w - 2).step_by(3) {
        for yy in (surface[x] as usize + 2)..h - 2 {
            let here = world.get(x, yy);
            let below = world.get(x, yy + 1);
            if here == Material::Empty && (below == Material::Rock || below == Material::Soil) {
                floors.push((x, yy));
                break; // one candidate per column
            }
        }
    }
    if !floors.is_empty() {
        let grove_count = (w / 28).max(3);
        for g in 0..grove_count {
            let (fx, fy) = floors[(rng.next() as usize) % floors.len()];
            let giant = g < 2; // the first two groves are giants
            let height = if giant { rng.range(14, 24) } else { rng.range(4, 10) } as usize;
            let cap_rx = if giant { rng.range(8, 14) } else { rng.range(3, 7) };
            let cap_ry = (cap_rx / 2).max(2);
            // stem
            for dy in 0..height {
                if fy > dy {
                    let sy = fy - dy;
                    if world.get(fx, sy) == Material::Empty {
                        set(world, fx, sy, Material::MushroomFlesh, &mut rng);
                    }
                    if giant && fx + 1 < w && world.get(fx + 1, sy) == Material::Empty {
                        set(world, fx + 1, sy, Material::MushroomFlesh, &mut rng);
                    }
                }
            }
            // elliptical cap, only into open air
            let top = fy as i32 - height as i32;
            for dy in -cap_ry..=0 {
                for dx in -cap_rx..=cap_rx {
                    let f = (dx * dx) as f32 / (cap_rx * cap_rx).max(1) as f32
                        + (dy * dy) as f32 / (cap_ry * cap_ry).max(1) as f32;
                    if f > 1.0 {
                        continue;
                    }
                    let (cx2, cy2) = (fx as i32 + dx, top + dy);
                    if cx2 < 0 || cy2 < 0 || cx2 as usize >= w || cy2 as usize >= h {
                        continue;
                    }
                    if world.get(cx2 as usize, cy2 as usize) == Material::Empty {
                        set(world, cx2 as usize, cy2 as usize, Material::MushroomFlesh, &mut rng);
                    }
                }
            }
        }
    }

    // 3e. spore pockets: gas blobs in cave air; they rise and pool by themselves
    let mut placed_spores = 0;
    for _ in 0..3000 {
        if placed_spores >= (w / 20).max(5) {
            break;
        }
        let x = rng.range(4, w as i32 - 4);
        let yy = rng.range(surface[x as usize] + 4, h as i32 - 4);
        if world.get(x as usize, yy as usize) != Material::Empty {
            continue;
        }
        blob(world, &mut rng, x, yy, rng.range(2, 4), Material::SporeGas);
        placed_spores += 1;
    }
```
Note: `set`/`blob` write via `Cell::new` semantics after Task 1 — update `set` in worldgen.rs to:
```rust
fn set(world: &mut World, x: usize, y: usize, m: Material, rng: &mut GenRng) {
    let i = y * world.width + x;
    world.cells[i] = Cell::new(m, (rng.next() & 3) as u8);
}
```
(and adjust its import to `use crate::cell::{Cell, Material};` if not already).

- [ ] **Step 4: Run the FULL suite**

Run: `cargo test -p sandgun-core`
Expected: all pass — including `generation_is_deterministic` and `generated_world_fully_settles` (spore pockets rise then rest; nothing burns at generation). If a presence threshold misses at 256×192/seed 7, tune counts/radii in 3b–3e — thresholds are the spec.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: fungal worldgen — soil, veins, mushroom groves, spore pockets (M1a task 5)"
```

---

### Task 6: Hot-reloadable params + debug palette + browser acceptance

**Files:**
- Modify: `crates/sandgun-wasm/src/lib.rs` (add `set_param`)
- Create: `web/public/params.json`
- Create: `web/src/params.js`
- Modify: `web/src/scene.js` (extend `M`)
- Modify: `web/src/input.js` (extend palette keys, add `p` reload)
- Modify: `web/src/main.js` (load params at boot)

**Interfaces:**
- Consumes: everything above; `WasmWorld`.
- Produces: `WasmWorld::set_param(index: u32, value: f32)`; `web/public/params.json` (name → value, names mirroring `params.rs` indices via the map in `params.js`); `loadParams(world)` fetched at boot and re-fetched on `P` keypress (tune the JSON, press P, no rebuild); debug palette gains `5` soil, `6` mycelium, `7` flesh, `8` spore gas, `9` acid, `F` fire.

- [ ] **Step 1: Implement wasm + JS**

Add to `crates/sandgun-wasm/src/lib.rs` inside `impl WasmWorld`:
```rust
pub fn set_param(&mut self, index: u32, value: f32) {
    if (index as usize) < sandgun_core::params::P_COUNT {
        self.inner.params.values[index as usize] = value;
    }
}
```

`web/public/params.json`:
```json
{
  "fire_lifetime": 40,
  "smoke_lifetime": 120,
  "smoke_emit": 0.2,
  "fire_flicker": 0.35,
  "flam_oil": 0.65,
  "flam_mycelium": 0.22,
  "flam_flesh": 0.06,
  "flam_sporegas": 1.0,
  "fuel_oil": 90,
  "fuel_mycelium": 130,
  "fuel_flesh": 220,
  "fuel_sporegas": 6,
  "acid_etch": 0.35,
  "acid_etch_rock": 0.04
}
```

`web/src/params.js`:
```js
// Index map mirrors crates/sandgun-core/src/params.rs — keep in sync.
const INDEX = {
  fire_lifetime: 0, smoke_lifetime: 1, smoke_emit: 2, fire_flicker: 3,
  flam_oil: 4, flam_mycelium: 5, flam_flesh: 6, flam_sporegas: 7,
  fuel_oil: 8, fuel_mycelium: 9, fuel_flesh: 10, fuel_sporegas: 11,
  acid_etch: 12, acid_etch_rock: 13,
};

export async function loadParams(world) {
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
}
```

`web/src/scene.js`:
```js
// Material ids must match sandgun-core cell.rs
export const M = {
  EMPTY: 0, ROCK: 1, SAND: 2, WATER: 3, OIL: 4,
  SOIL: 5, MYCELIUM: 6, FLESH: 7, SPOREGAS: 8,
  SMOKE: 9, ASH: 10, ACID: 11, FIRE: 12,
};
```

In `web/src/input.js`, replace `KEYS`/`NAMES` and extend the keydown handler:
```js
const KEYS = {
  '1': M.SAND, '2': M.WATER, '3': M.OIL, '4': M.ROCK, '5': M.SOIL,
  '6': M.MYCELIUM, '7': M.FLESH, '8': M.SPOREGAS, '9': M.ACID,
  'f': M.FIRE, '0': M.EMPTY, 'e': M.EMPTY,
};
const NAMES = ['erase', 'rock', 'sand', 'water', 'oil', 'soil', 'mycelium',
  'flesh', 'spores', 'smoke', 'ash', 'acid', 'FIRE'];
```
and in the handler add:
```js
    if (k === 'p') input.reloadParams = true;
```
(add `reloadParams: false` to the input object). In `applyInput`, handle it like regen:
```js
  if (input.reloadParams) {
    input.reloadParams = false;
    input.onReloadParams?.();
  }
```

In `web/src/main.js`, after creating `world`:
```js
import { loadParams } from './params.js';
await loadParams(world);
input.onReloadParams = () => loadParams(world);
```
(the `input.onReloadParams` line goes after `attachInput`).

Rebuild wasm: `wasm-pack build crates/sandgun-wasm --release --target web --out-dir ../../web/src/pkg`

- [ ] **Step 2: Verify in the browser (Playwright as in prior tasks)**

Load the app. Expected: a fungal world — brown soil crust under the surface, pale-violet veins in it, cream mushrooms standing in caves (a couple huge), green gas pooled at cave ceilings. Then: press `F`, click a mycelium vein — fire crawls along it leaving grey ash, smoke rises; the fire eventually dies and (debug on) chunks re-sleep. Click spore gas with fire — the pocket flashes. `9` + drag melts terrain. Edit `web/public/params.json` (set `flam_mycelium` to 0), press `P`, relight — vein doesn't catch. Restore the value, press `P` again.

- [ ] **Step 3: Performance check**

With debug on, set a large fire (drag `F` across several veins and a grove). Expected: FPS stays ≥60 during the burn on the Mac; world settles to 0 cells after.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: hot-reload params, fungal debug palette (M1a task 6)"
```

---

## Self-review notes

- Spec coverage (M1a slice of the design spec): materials ✓ (T1), gases ✓ (T2), fire/fuses/detonation/extinguish ✓ (T3), acid ✓ (T4), fungal worldgen ✓ (T5), hot-reload dev loop ✓ (T6). Deliberately deferred to M1b: growth frontier/lifecycle, spore round + gun/projectiles, pixels-as-particles + explosion displacement, 1024×2048 world + camera, oil→sludge re-flavor.
- Fire tests are probabilistic but seeded — the sim RNG is deterministic, so outcomes are reproducible; generous step budgets make thresholds robust to param tweaks.
- Known simplification: `initial_aux` uses static defaults, so params changed at runtime affect ignition fuel and emitted smoke but not the lifetime of hand-painted Fire/Smoke/Acid cells. Acceptable for a debug tool; noted here so nobody files it as a bug.
- `cells_processed` counting changed subtly (static solids in active chunks no longer counted via early return but via the `_ => {}` arm) — semantics identical to M0.
