# SANDGUN M1b — "The Gun" Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Aim and shoot the fungal world. A projectile is a physics event: it flies over the grid, collides with terrain, and applies a payload on impact — kinetic craters that eject debris as free-flying particles, incendiary rounds that light fuses, acid rounds that melt terrain, spore rounds that plant mycelium. The grin test: 60 seconds of shooting the world we built in M1a should make Lex grin.

**Architecture:** `sandgun-core` gains two entity layers that live *above* the cell grid and are updated each `step()`: **projectiles** (fast-moving payload carriers, ray-marched for collision) and **particles** (cells temporarily out of the grid, flying with gravity, resettling on landing — the "pixels-as-particles" trick). Payloads mutate the grid via a shared set of ops (carve, inject, ignite, explode). Rendering stamps projectiles and particles into the RGBA buffer after the grid, so it's still one texture upload. The web app adds aim/fire/ammo input. Camera and the bigger 1024×2048 world are deferred to a later slice — M1b plays at the existing 640×384.

**Tech Stack:** unchanged (Rust stable, wasm-bindgen/wasm-pack via `scripts/build-wasm.sh`, Vite, WebGL2).

## Global Constraints

- Cell stays 4 bytes; flags bit 7 reserved (M2 rigid bodies); FLAG_BURNING = bit 1.
- NO per-cell temperature. Fire remains cell-state (from M1a).
- Chunk sleeping is sacred: firing/impacts must `wake()` affected cells; when no projectiles/particles are live and the grid is settled, `cells_processed` returns to 0. Projectile/particle updates only cost work while entities exist.
- Sim logic (projectiles, particles, payloads) lives only in `sandgun-core`. `sandgun-wasm` stays glue-only. Input/render glue in `web/`.
- Coordinates: cell grid is integer; projectiles/particles use `f32` world coordinates where (0,0) is the top-left of cell (0,0) and +y is down (matches gravity). Cell for a position is `(x.floor() as isize, y.floor() as isize)`.
- Material ids unchanged from M1a: `Empty=0 Rock=1 Sand=2 Water=3 Oil=4 Soil=5 Mycelium=6 MushroomFlesh=7 SporeGas=8 Smoke=9 Ash=10 Acid=11 Fire=12`.
- Determinism: reuse `World::next_rand`/`chance` for any randomness (ejecta spread). No `f32` NaN paths — clamp/validate velocities.
- All commits end with: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` (second `-m`). Run cargo from repo root (Homebrew rust is fine for native tests). Rebuild wasm with `./scripts/build-wasm.sh`.
- Work on a branch: `git checkout -b m1b-the-gun` before Task 1.

---

### Task 1: Particle system (pixels-as-particles)

**Files:**
- Create: `crates/sandgun-core/src/particle.rs`
- Modify: `crates/sandgun-core/src/lib.rs` (add `pub mod particle;`)
- Modify: `crates/sandgun-core/src/world.rs` (add `particles` field; `spawn_particle`; call `update_particles` in `step`)
- Create: `crates/sandgun-core/tests/particles.rs`

**Interfaces:**
- Consumes: `World` grid/`idx`/`in_bounds`/`material_at`/`wake`/`cells`, `Material`, `Cell::new`.
- Produces: `particle::Particle { x: f32, y: f32, vx: f32, vy: f32, material: u8 }`; `World.particles: Vec<Particle>` (pub(crate)); `World::spawn_particle(x, y, vx, vy, material)`; `World::particle_count() -> usize`; `World::update_particles()` — integrates all particles (gravity `PARTICLE_GRAVITY`, ray-march one cell-step at a time), resettles a particle into the grid when its next cell is blocked (writes its material into the last empty cell it occupied, waking it), and drops particles that leave the world. Called from `step()` after the cell sweep. Particles over Empty/gas keep flying; a particle whose target cell is non-empty & non-gas settles.

- [ ] **Step 1: Write the failing tests**

`crates/sandgun-core/tests/particles.rs`:
```rust
use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn particle_falls_and_settles_on_the_floor() {
    let mut w = World::new(64, 64);
    // rock floor at y=60
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Sand as u8);
    }
    w.spawn_particle(10.0, 5.0, 0.0, 0.0, Material::Sand as u8);
    assert_eq!(w.particle_count(), 1);
    for _ in 0..400 {
        w.step();
    }
    assert_eq!(w.particle_count(), 0, "particle must resettle, not fly forever");
    // it should have become a grid cell resting on the floor (row 59, just above the sand)
    assert_eq!(w.get(10, 59), Material::Sand, "particle resettled onto the floor");
}

#[test]
fn particle_with_sideways_velocity_lands_offset() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8);
    }
    w.spawn_particle(10.0, 5.0, 3.0, 0.0, Material::Sand as u8); // flung right
    for _ in 0..400 {
        w.step();
    }
    assert_eq!(w.particle_count(), 0);
    // landed to the right of x=10
    let landed = (11..64).any(|x| w.get(x, 59) == Material::Sand);
    assert!(landed, "a particle flung sideways should land to the right of its origin");
}

#[test]
fn particle_leaving_the_world_is_dropped() {
    let mut w = World::new(64, 64);
    w.spawn_particle(32.0, 5.0, 0.0, -50.0, Material::Sand as u8); // flung up and out
    for _ in 0..20 {
        w.step();
    }
    assert_eq!(w.particle_count(), 0, "particle that exits the world is dropped, not kept");
}

#[test]
fn particles_do_not_keep_the_world_awake_forever() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8);
    }
    w.spawn_particle(10.0, 5.0, 0.0, 0.0, Material::Sand as u8);
    for _ in 0..400 {
        w.step();
    }
    w.step();
    assert_eq!(w.particle_count(), 0);
    assert_eq!(w.cells_processed, 0, "once particles resettle and the grid settles, work is 0");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sandgun-core --test particles`
Expected: FAIL — `spawn_particle`/`particle` module don't exist.

- [ ] **Step 3: Implement particles**

`crates/sandgun-core/src/particle.rs`:
```rust
#[derive(Clone, Copy)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub material: u8,
}
```

`crates/sandgun-core/src/lib.rs`: add `pub mod particle;`.

In `crates/sandgun-core/src/world.rs`:
- import: `use crate::particle::Particle;`
- add constants near the top: `pub const PARTICLE_GRAVITY: f32 = 0.35; const PARTICLE_MAX_SPEED: f32 = 8.0;`
- add field to `World`: `pub(crate) particles: Vec<Particle>,` and init `particles: Vec::new(),` in `new()`.
- add methods inside `impl World`:
```rust
pub fn spawn_particle(&mut self, x: f32, y: f32, vx: f32, vy: f32, material: u8) {
    self.particles.push(Particle { x, y, vx, vy, material });
}

pub fn particle_count(&self) -> usize {
    self.particles.len()
}

/// A cell a particle can fly through without settling.
fn particle_passable(&self, x: isize, y: isize) -> bool {
    let m = self.material_at(x, y);
    m == Material::Empty || m.is_gas()
}

pub(crate) fn update_particles(&mut self) {
    if self.particles.is_empty() {
        return;
    }
    let mut survivors: Vec<Particle> = Vec::with_capacity(self.particles.len());
    let existing = std::mem::take(&mut self.particles);
    for mut p in existing {
        p.vy += PARTICLE_GRAVITY;
        // clamp speed to keep the ray-march bounded and avoid tunneling
        p.vx = p.vx.clamp(-PARTICLE_MAX_SPEED, PARTICLE_MAX_SPEED);
        p.vy = p.vy.clamp(-PARTICLE_MAX_SPEED, PARTICLE_MAX_SPEED);
        let steps = p.vx.abs().max(p.vy.abs()).ceil().max(1.0) as i32;
        let (sx, sy) = (p.vx / steps as f32, p.vy / steps as f32);
        let mut settled = false;
        let mut last_x = p.x;
        let mut last_y = p.y;
        for _ in 0..steps {
            let nx = p.x + sx;
            let ny = p.y + sy;
            let (cx, cy) = (nx.floor() as isize, ny.floor() as isize);
            if !self.in_bounds(cx, cy) {
                // left the world: if it went out the sides/top/bottom, drop it
                settled = true; // "settled" here means "remove from list"
                last_x = f32::NAN; // sentinel: don't write to grid
                break;
            }
            if self.particle_passable(cx, cy) {
                p.x = nx;
                p.y = ny;
                last_x = nx;
                last_y = ny;
            } else {
                // blocked: resettle into the last passable cell we occupied
                settled = true;
                break;
            }
        }
        if settled {
            if last_x.is_finite() {
                let (cx, cy) = (last_x.floor() as isize, last_y.floor() as isize);
                if self.in_bounds(cx, cy) && self.material_at(cx, cy) == Material::Empty {
                    let (ux, uy) = (cx as usize, cy as usize);
                    let shade = (self.next_rand() & 3) as u8;
                    let i = self.idx(ux, uy);
                    self.cells[i] = Cell::new(Material::from_u8(p.material), shade);
                    self.wake(ux, uy);
                }
                // if the last cell isn't empty (rare — landed on a gas that filled), drop silently
            }
        } else {
            survivors.push(p);
        }
    }
    self.particles = survivors;
}
```
- in `step()`, after the cell sweep loop (right before the method ends), add: `self.update_particles();`

- [ ] **Step 4: Run the FULL suite**

Run: `cargo test -p sandgun-core`
Expected: all pass (old + 4 new). If `particle_falls_and_settles_on_the_floor` lands on the wrong row, check that resettle writes the *last passable* cell (the one just above the blocker), not the blocked cell.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: pixels-as-particles — flying cells that resettle (M1b task 1)"
```

---

### Task 2: Projectiles — ray-march flight and collision

**Files:**
- Create: `crates/sandgun-core/src/projectile.rs`
- Modify: `crates/sandgun-core/src/lib.rs` (add `pub mod projectile;`)
- Modify: `crates/sandgun-core/src/world.rs` (add `projectiles` field; `fire`; `update_projectiles` in `step`; impact dispatch stub)
- Create: `crates/sandgun-core/tests/projectiles.rs`

**Interfaces:**
- Consumes: Task 1 world plumbing, `material_at`, `in_bounds`.
- Produces: `projectile::{Ammo, Projectile}` where `Ammo` is a `#[repr(u8)]` enum `{ Kinetic=0, Incendiary=1, Acid=2, Spore=3 }` (with `from_u8`), and `Projectile { x, y, vx, vy, ammo: Ammo, alive: bool }`; `World.projectiles: Vec<Projectile>` (pub(crate)); `World::fire(x, y, vx, vy, ammo: u8)`; `World::projectile_count()`; a read accessor `World::projectiles_xy() -> Vec<f32>` (flat [x0,y0,x1,y1,...] for the renderer); `World::update_projectiles()` — ray-marches each projectile one cell at a time through passable cells (Empty/gas), and on hitting a non-passable cell calls `self.on_impact(cx, cy, ammo)` (stub in this task: just carve nothing, mark dead), removing dead/off-world projectiles. Called from `step()` BEFORE `update_particles()` (impacts can spawn particles that then integrate same frame).

- [ ] **Step 1: Write the failing tests**

`crates/sandgun-core/tests/projectiles.rs`:
```rust
use sandgun_core::cell::Material;
use sandgun_core::projectile::Ammo;
use sandgun_core::world::World;

#[test]
fn projectile_flies_through_empty_and_stops_at_a_wall() {
    let mut w = World::new(64, 64);
    for y in 0..64 {
        w.paint(40, y, 0, Material::Rock as u8); // vertical wall at x=40
    }
    w.fire(5.0, 32.0, 6.0, 0.0, Ammo::Kinetic as u8); // flying right at 6 cells/frame
    assert_eq!(w.projectile_count(), 1);
    for _ in 0..30 {
        w.step();
    }
    assert_eq!(w.projectile_count(), 0, "projectile must die on impact, not persist");
}

#[test]
fn fast_projectile_does_not_tunnel_through_a_thin_wall() {
    let mut w = World::new(64, 64);
    for y in 0..64 {
        w.paint(30, y, 0, Material::Rock as u8); // 1-cell-thick wall
    }
    // very fast: 20 cells/frame would tunnel a 1-cell wall without ray-marching
    w.fire(2.0, 32.0, 20.0, 0.0, Ammo::Kinetic as u8);
    w.step();
    // the wall must still be intact directly behind where it should have stopped;
    // with a stub impact (no carve), the wall is fully intact and the projectile is gone
    assert_eq!(w.projectile_count(), 0, "projectile resolved its impact on the wall");
    assert_eq!(w.get(30, 32), Material::Rock, "thin wall not tunneled (stub impact carves nothing)");
    // nothing past the wall was disturbed
    assert!((31..64).all(|x| w.get(x, 32) == Material::Empty || x == 40));
}

#[test]
fn projectile_leaving_the_world_is_dropped() {
    let mut w = World::new(64, 64);
    w.fire(60.0, 32.0, 8.0, 0.0, Ammo::Kinetic as u8); // fired toward the right edge
    for _ in 0..10 {
        w.step();
    }
    assert_eq!(w.projectile_count(), 0);
}

#[test]
fn projectiles_alone_do_not_keep_the_world_awake() {
    let mut w = World::new(64, 64);
    for y in 0..64 {
        w.paint(40, y, 0, Material::Rock as u8);
    }
    w.fire(5.0, 32.0, 6.0, 0.0, Ammo::Kinetic as u8);
    for _ in 0..30 {
        w.step();
    }
    w.step();
    assert_eq!(w.projectile_count(), 0);
    assert_eq!(w.cells_processed, 0, "stub impact leaves the grid settled");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sandgun-core --test projectiles`
Expected: FAIL — `projectile` module / `fire` don't exist.

- [ ] **Step 3: Implement projectiles**

`crates/sandgun-core/src/projectile.rs`:
```rust
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
```

`crates/sandgun-core/src/lib.rs`: add `pub mod projectile;`.

In `crates/sandgun-core/src/world.rs`:
- import: `use crate::projectile::{Ammo, Projectile};`
- add field `pub(crate) projectiles: Vec<Projectile>,` and init in `new()`.
- add methods:
```rust
pub fn fire(&mut self, x: f32, y: f32, vx: f32, vy: f32, ammo: u8) {
    self.projectiles.push(Projectile {
        x, y, vx, vy, ammo: Ammo::from_u8(ammo), alive: true,
    });
}

pub fn projectile_count(&self) -> usize {
    self.projectiles.len()
}

/// Flat [x0,y0,x1,y1,...] world coords of live projectiles, for rendering.
pub fn projectiles_xy(&self) -> Vec<f32> {
    let mut v = Vec::with_capacity(self.projectiles.len() * 2);
    for p in &self.projectiles {
        v.push(p.x);
        v.push(p.y);
    }
    v
}

pub(crate) fn update_projectiles(&mut self) {
    if self.projectiles.is_empty() {
        return;
    }
    let existing = std::mem::take(&mut self.projectiles);
    let mut survivors = Vec::with_capacity(existing.len());
    for mut p in existing {
        let steps = p.vx.abs().max(p.vy.abs()).ceil().max(1.0) as i32;
        let (sx, sy) = (p.vx / steps as f32, p.vy / steps as f32);
        for _ in 0..steps {
            let nx = p.x + sx;
            let ny = p.y + sy;
            let (cx, cy) = (nx.floor() as isize, ny.floor() as isize);
            if !self.in_bounds(cx, cy) {
                p.alive = false;
                break;
            }
            let m = self.material_at(cx, cy);
            if m == Material::Empty || m.is_gas() {
                p.x = nx;
                p.y = ny;
            } else {
                self.on_impact(cx, cy, p.ammo);
                p.alive = false;
                break;
            }
        }
        if p.alive {
            survivors.push(p);
        }
    }
    self.projectiles = survivors;
}

fn on_impact(&mut self, _cx: isize, _cy: isize, _ammo: Ammo) {
    // Task 3 fills this in per ammo type.
}
```
- in `step()`, add BEFORE the `self.update_particles();` line: `self.update_projectiles();`

- [ ] **Step 4: Run the FULL suite**

Run: `cargo test -p sandgun-core`
Expected: all pass (+4).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: projectiles — ray-marched flight and collision (M1b task 2)"
```

---

### Task 3: Payloads — the bullet becomes a physics event

**Files:**
- Modify: `crates/sandgun-core/src/world.rs` (implement `on_impact` + payload helpers)
- Modify: `crates/sandgun-core/src/params.rs` (add payload tuning params)
- Create: `crates/sandgun-core/tests/payloads.rs`

**Interfaces:**
- Consumes: Tasks 1–2, `ignite_neighbors`/`FLAG_BURNING` (M1a), `spawn_particle`, `chance`, `next_rand`.
- Produces: `on_impact(cx, cy, ammo)` dispatch and helpers — `carve_crater(cx, cy, radius, ejecta_frac)` (clears solid/powder/liquid cells in radius; a fraction of cleared *solid/powder* cells relaunch as particles with outward velocity; liquids just clear), `ignite_blast(cx, cy, radius)` (sets FLAG_BURNING on flammable cells + spawns a few Fire cells), `inject_blob(cx, cy, radius, material)` (fills Empty cells in radius, and for Acid/Spore overwrites soft cells). Ammo mapping: Kinetic→carve_crater; Incendiary→small carve + ignite_blast; Acid→inject_blob(Acid); Spore→inject_blob(Mycelium) + a puff of SporeGas. New params: `P_KINETIC_RADIUS, P_KINETIC_EJECTA (0..1), P_INCENDIARY_RADIUS, P_ACID_BLOB_RADIUS, P_SPORE_BLOB_RADIUS` appended (bump `P_COUNT`; add to `Default` and to `web/public/params.json` + `params.js` in Task 6).

- [ ] **Step 1: Write the failing tests**

`crates/sandgun-core/tests/payloads.rs`:
```rust
use sandgun_core::cell::Material;
use sandgun_core::projectile::Ammo;
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
fn kinetic_round_blasts_a_crater_and_throws_debris() {
    let mut w = World::new(64, 64);
    // solid sand block
    for x in 20..44 {
        for y in 20..44 {
            w.paint(x, y, 0, Material::Sand as u8);
        }
    }
    let sand0 = count(&w, Material::Sand);
    w.fire(2.0, 32.0, 12.0, 0.0, Ammo::Kinetic as u8); // into the block from the left
    w.step(); // impact this frame
    let sand1 = count(&w, Material::Sand);
    assert!(sand1 < sand0, "kinetic impact must remove sand (a crater)");
    assert!(w.particle_count() > 0, "some blasted sand becomes flying debris");
    for _ in 0..400 {
        w.step();
    }
    // debris resettles; world eventually calms
    assert_eq!(w.particle_count(), 0);
}

#[test]
fn incendiary_round_lights_oil() {
    let mut w = World::new(64, 64);
    for x in 20..44 {
        for y in 30..40 {
            w.paint(x, y, 0, Material::Oil as u8);
        }
    }
    w.fire(2.0, 34.0, 12.0, 0.0, Ammo::Incendiary as u8);
    for _ in 0..200 {
        w.step();
    }
    // fire consumes oil over time; a good chunk should be gone (burned to empty)
    assert!(count(&w, Material::Oil) < 24 * 10, "incendiary round ignited the oil pool");
}

#[test]
fn acid_round_deposits_acid() {
    let mut w = World::new(64, 64);
    for x in 20..44 {
        for y in 30..44 {
            w.paint(x, y, 0, Material::Rock as u8);
        }
    }
    w.fire(2.0, 29.0, 12.0, -1.0, Ammo::Acid as u8);
    w.step();
    assert!(count(&w, Material::Acid) > 0, "acid round leaves acid at the impact");
}

#[test]
fn spore_round_plants_mycelium() {
    let mut w = World::new(64, 64);
    for x in 20..44 {
        w.paint(x, 40, 0, Material::Soil as u8);
    }
    w.fire(2.0, 39.0, 12.0, 0.0, Ammo::Spore as u8);
    w.step();
    assert!(count(&w, Material::Mycelium) > 0, "spore round plants mycelium at impact");
}

#[test]
fn firing_into_empty_world_settles_after_impacts_resolve() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8);
    }
    for _ in 0..5 {
        w.fire(2.0, 30.0, 10.0, 0.0, Ammo::Kinetic as u8);
        for _ in 0..20 {
            w.step();
        }
    }
    for _ in 0..500 {
        w.step();
    }
    w.step();
    assert_eq!(w.projectile_count(), 0);
    assert_eq!(w.particle_count(), 0);
    assert_eq!(w.cells_processed, 0, "world must return to rest after the dust settles");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p sandgun-core --test payloads`
Expected: FAIL — `on_impact` is a stub, so no craters/fire/acid/mycelium appear.

- [ ] **Step 3: Implement payloads**

Add params to `crates/sandgun-core/src/params.rs` (append constants, bump `P_COUNT`, set defaults):
```rust
pub const P_KINETIC_RADIUS: usize = 14;
pub const P_KINETIC_EJECTA: usize = 15;   // 0..1 fraction of carved solids that fly
pub const P_INCENDIARY_RADIUS: usize = 16;
pub const P_ACID_BLOB_RADIUS: usize = 17;
pub const P_SPORE_BLOB_RADIUS: usize = 18;
pub const P_COUNT: usize = 19;
```
in `Default`:
```rust
v[P_KINETIC_RADIUS] = 5.0;
v[P_KINETIC_EJECTA] = 0.35;
v[P_INCENDIARY_RADIUS] = 3.0;
v[P_ACID_BLOB_RADIUS] = 3.0;
v[P_SPORE_BLOB_RADIUS] = 4.0;
```

In `crates/sandgun-core/src/world.rs`, extend the params import with the new consts and replace `on_impact`, adding helpers:
```rust
fn on_impact(&mut self, cx: isize, cy: isize, ammo: Ammo) {
    match ammo {
        Ammo::Kinetic => {
            let r = self.params.values[P_KINETIC_RADIUS] as i32;
            let ej = self.params.values[P_KINETIC_EJECTA];
            self.carve_crater(cx, cy, r, ej);
        }
        Ammo::Incendiary => {
            let r = self.params.values[P_INCENDIARY_RADIUS] as i32;
            self.carve_crater(cx, cy, (r - 1).max(1), 0.15);
            self.ignite_blast(cx, cy, r);
        }
        Ammo::Acid => {
            let r = self.params.values[P_ACID_BLOB_RADIUS] as i32;
            self.inject_blob(cx, cy, r, Material::Acid);
        }
        Ammo::Spore => {
            let r = self.params.values[P_SPORE_BLOB_RADIUS] as i32;
            self.inject_blob(cx, cy, r, Material::Mycelium);
            self.inject_blob(cx, cy - r, (r / 2).max(1), Material::SporeGas); // a puff above
        }
    }
}

fn carve_crater(&mut self, cx: isize, cy: isize, radius: i32, ejecta_frac: f32) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            let (x, y) = (cx + dx, cy + dy);
            if !self.in_bounds(x, y) {
                continue;
            }
            let m = self.material_at(x, y);
            if m == Material::Empty || m == Material::Rock {
                continue; // rock resists kinetic rounds (keeps caves stable)
            }
            let (ux, uy) = (x as usize, y as usize);
            let i = self.idx(ux, uy);
            // throw a fraction of powders/solids outward as debris; clear the rest
            if (m.is_powder() || m == Material::Mycelium || m == Material::MushroomFlesh)
                && self.chance(ejecta_frac)
            {
                let ang = (self.next_rand() & 255) as f32 / 255.0 * std::f32::consts::TAU;
                let spd = 2.0 + (self.next_rand() & 63) as f32 / 32.0;
                let mat = self.cells[i].material;
                self.spawn_particle(x as f32 + 0.5, y as f32 + 0.5, ang.cos() * spd, ang.sin() * spd - 1.0, mat);
            }
            self.cells[i] = Cell::default();
            self.wake(ux, uy);
        }
    }
}

fn ignite_blast(&mut self, cx: isize, cy: isize, radius: i32) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            let (x, y) = (cx + dx, cy + dy);
            if !self.in_bounds(x, y) {
                continue;
            }
            let (ux, uy) = (x as usize, y as usize);
            let i = self.idx(ux, uy);
            let m = Material::from_u8(self.cells[i].material);
            if self.params.flammability(m) > 0.0 {
                self.cells[i].flags |= FLAG_BURNING;
                self.cells[i].aux = self.params.fuel(m);
                self.stamp[i] = self.frame_u8();
                self.wake(ux, uy);
            } else if m == Material::Empty && self.chance(0.3) {
                self.cells[i] = Cell::new(Material::Fire, (self.next_rand() & 3) as u8);
                self.stamp[i] = self.frame_u8();
                self.wake(ux, uy);
            }
        }
    }
}

fn inject_blob(&mut self, cx: isize, cy: isize, radius: i32, material: Material) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            let (x, y) = (cx + dx, cy + dy);
            if !self.in_bounds(x, y) {
                continue;
            }
            let (ux, uy) = (x as usize, y as usize);
            let i = self.idx(ux, uy);
            let dst = Material::from_u8(self.cells[i].material);
            // fill empty; acid/spore also eat into soft organics/soil, not rock
            let soft = matches!(dst, Material::Soil | Material::Sand | Material::Mycelium);
            if dst == Material::Empty || soft {
                self.cells[i] = Cell::new(material, (self.next_rand() & 3) as u8);
                self.wake(ux, uy);
            }
        }
    }
}
```

- [ ] **Step 4: Run the FULL suite**

Run: `cargo test -p sandgun-core`
Expected: all pass (+5). Tune payload params/thresholds if a count assertion is close, noting changes.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: ammo payloads — craters, ejecta, ignition, acid, spores (M1b task 3)"
```

---

### Task 4: WASM surface + projectile/particle rendering

**Files:**
- Modify: `crates/sandgun-wasm/src/lib.rs` (add `fire`, `particle_count`, `projectile_count`, `render` already stamps entities)
- Modify: `crates/sandgun-core/src/world.rs` (extend `render_rgba` to stamp particles + projectiles into the buffer)
- Modify: `crates/sandgun-core/tests/materials.rs` or a small render test (assert a particle pixel lands in the RGBA buffer)

**Interfaces:**
- Consumes: all above; `render_rgba` from M0/M1a.
- Produces: `WasmWorld::fire(x, y, vx, vy, ammo)` passthrough; `WasmWorld::projectile_count()/particle_count()`; `render_rgba` draws each particle and projectile as a bright pixel at its cell (projectiles a hot near-white, particles their material color) AFTER the grid fill, so they show over terrain in the single texture blit.

- [ ] **Step 1: Write the failing test**

Append to `crates/sandgun-core/tests/particles.rs`:
```rust
#[test]
fn render_stamps_a_flying_particle_into_the_buffer() {
    let mut w = World::new(64, 64);
    // a particle mid-air over empty space
    w.spawn_particle(10.0, 10.0, 0.0, 0.0, Material::Sand as u8);
    w.render_rgba();
    let px = w.rgba();
    let o = (10 * 64 + 10) * 4;
    // empty background is [26,24,32]; a stamped sand particle must differ
    assert!(
        px[o] != 26 || px[o + 1] != 24 || px[o + 2] != 32,
        "a flying particle must be drawn into the render buffer"
    );
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p sandgun-core --test particles render_stamps_a_flying_particle_into_the_buffer`
Expected: FAIL — render doesn't draw particles yet.

- [ ] **Step 3: Implement**

At the END of `render_rgba` in `crates/sandgun-core/src/world.rs` (after the grid loop), add:
```rust
    // stamp particles (their material color) and projectiles (hot tracer) over the grid
    for p in &self.particles {
        let (cx, cy) = (p.x.floor() as isize, p.y.floor() as isize);
        if self.in_bounds(cx, cy) {
            let [r, g, b] = Material::from_u8(p.material).base_color();
            let o = (cy as usize * self.width + cx as usize) * 4;
            self.rgba[o] = r;
            self.rgba[o + 1] = g;
            self.rgba[o + 2] = b;
            self.rgba[o + 3] = 255;
        }
    }
    for p in &self.projectiles {
        let (cx, cy) = (p.x.floor() as isize, p.y.floor() as isize);
        if self.in_bounds(cx, cy) {
            let o = (cy as usize * self.width + cx as usize) * 4;
            self.rgba[o] = 255;
            self.rgba[o + 1] = 240;
            self.rgba[o + 2] = 200;
            self.rgba[o + 3] = 255;
        }
    }
```

Add to `crates/sandgun-wasm/src/lib.rs` inside `impl WasmWorld`:
```rust
pub fn fire(&mut self, x: f32, y: f32, vx: f32, vy: f32, ammo: u8) {
    self.inner.fire(x, y, vx, vy, ammo);
}
pub fn projectile_count(&self) -> usize {
    self.inner.projectile_count()
}
pub fn particle_count(&self) -> usize {
    self.inner.particle_count()
}
```

- [ ] **Step 4: Run suite + build wasm**

Run: `cargo test -p sandgun-core` (all pass) then `./scripts/build-wasm.sh` (succeeds).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: wasm fire() + render projectiles and particles (M1b task 4)"
```

---

### Task 5: Aim, fire, and ammo-select input

**Files:**
- Create: `web/src/gun.js`
- Modify: `web/src/input.js` (track cursor for aim; right-mouse or a fire key; ammo keys; expose to main)
- Modify: `web/src/main.js` (wire the gun; fire from a muzzle point toward the cursor)

**Interfaces:**
- Consumes: `WasmWorld::fire`, existing input/camera-less coordinate mapping (canvas → world cell, already in input.js).
- Produces: `attachGun(canvas, worldW, worldH) -> gun` and `applyGun(gun, world)` (call once per frame in `frame()`). Firing model: the muzzle is fixed at bottom-center of the world for M1b (no avatar); on fire, compute the unit vector from muzzle to the cursor world-position, multiply by `GUN_SPEED` (≈10), and `world.fire(muzzleX, muzzleY, vx, vy, ammo)`. Fire on **left-click while a modifier is NOT held** would clash with paint — so gun fire is **right-click** (`contextmenu` prevented) or holding **Space + left-click**; ammo select keys `Z/X/C/V` = kinetic/incendiary/acid/spore; fire rate limited to one shot per ~6 frames while held. `gun.status` string ("ammo: incendiary") for the overlay. Paint (left-drag) stays as the debug tool.

- [ ] **Step 1: Implement the gun input**

`web/src/gun.js`:
```js
const AMMO = { z: 0, x: 1, c: 2, v: 3 }; // kinetic, incendiary, acid, spore
const AMMO_NAMES = ['kinetic', 'incendiary', 'acid', 'spore'];
const GUN_SPEED = 10;
const FIRE_COOLDOWN = 6; // frames

export function attachGun(canvas, worldW, worldH) {
  const gun = {
    ammo: 0, aimX: worldW / 2, aimY: 0, firing: false, cooldown: 0,
    muzzleX: worldW / 2, muzzleY: worldH - 2,
    get status() { return `gun: ${AMMO_NAMES[this.ammo]}`; },
  };
  const toWorld = (e) => {
    const r = canvas.getBoundingClientRect();
    gun.aimX = (e.clientX - r.left) / r.width * worldW;
    gun.aimY = (e.clientY - r.top) / r.height * worldH;
  };
  canvas.addEventListener('mousemove', toWorld);
  canvas.addEventListener('contextmenu', (e) => e.preventDefault());
  canvas.addEventListener('mousedown', (e) => { if (e.button === 2) { gun.firing = true; toWorld(e); } });
  window.addEventListener('mouseup', (e) => { if (e.button === 2) gun.firing = false; });
  window.addEventListener('keydown', (e) => {
    const k = e.key.toLowerCase();
    if (k in AMMO) gun.ammo = AMMO[k];
  });
  return gun;
}

export function applyGun(gun, world) {
  if (gun.cooldown > 0) gun.cooldown--;
  if (!gun.firing || gun.cooldown > 0) return;
  const dx = gun.aimX - gun.muzzleX;
  const dy = gun.aimY - gun.muzzleY;
  const len = Math.hypot(dx, dy) || 1;
  const vx = (dx / len) * GUN_SPEED;
  const vy = (dy / len) * GUN_SPEED;
  world.fire(gun.muzzleX, gun.muzzleY, vx, vy, gun.ammo);
  gun.cooldown = FIRE_COOLDOWN;
}
```

In `web/src/main.js`:
```js
import { attachGun, applyGun } from './gun.js';
// after attachInput:
const gun = attachGun(document.getElementById('view'), W, H);
// inside frame(), after applyInput(...):
applyGun(gun, world);
```

- [ ] **Step 2: Verify in the browser (Playwright)**

`./scripts/build-wasm.sh`, start the dev server. Right-click-drag to fire toward the cursor. Expected: tracer pixels fly from bottom-center toward the cursor; kinetic (`Z`) blasts craters in dirt/mushrooms and throws debris that arcs and lands; incendiary (`X`) sets fires (and lights mycelium veins → fuses from M1a); acid (`C`) leaves green acid that eats terrain; spore (`V`) plants violet mycelium. Rock resists kinetic craters. Left-drag still paints (debug). Verify no console errors.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: aim/fire/ammo-select gun input (M1b task 5)"
```

---

### Task 6: Gun params in the hot-reload pipeline + M1b acceptance

**Files:**
- Modify: `web/public/params.json` (add the 5 payload params)
- Modify: `web/src/params.js` (extend INDEX map)
- Modify: `web/src/overlay.js` (show `gun.status`)
- Modify: `web/src/main.js` (pass gun status to overlay)

**Interfaces:**
- Consumes: Task 3 params, Task 5 gun, the M1a hot-reload pipeline.
- Produces: the 5 payload params are hot-reloadable (`P` key); the overlay shows current ammo; M1b is playable and accepted.

- [ ] **Step 1: Extend the params pipeline**

Add to `web/public/params.json`:
```json
  "kinetic_radius": 5,
  "kinetic_ejecta": 0.35,
  "incendiary_radius": 3,
  "acid_blob_radius": 3,
  "spore_blob_radius": 4
```
Add to the `INDEX` map in `web/src/params.js`:
```js
  kinetic_radius: 14, kinetic_ejecta: 15, incendiary_radius: 16,
  acid_blob_radius: 17, spore_blob_radius: 18,
```
In `web/src/overlay.js`, append the gun status to the HUD line (pass `gun` into `drawOverlay` from `main.js` and add `· ${gun.status}` to the text).

- [ ] **Step 2: Rebuild + browser acceptance (the grin test)**

`./scripts/build-wasm.sh`, dev server. Run the M1b acceptance with Playwright + eyeballs:
- Fire each ammo at the fungal world; confirm the four distinct terrain-verbs (crater+debris / fire+fuses / acid melt / mycelium plant).
- **Chain-reaction beat:** incendiary into a spore pocket or along a mycelium vein triggers a cascade (fuse burns, spore gas detonates) — the core fantasy.
- Tune `params.json` (e.g. bump `kinetic_radius`), press `P`, confirm bigger craters without a rebuild.
- **Settling:** after a burst of fire and craters, stop; confirm (debug `D`) chunk boxes clear and `cells_processed` returns to 0 — projectiles/particles don't leak activity.
- **Perf:** sustained rapid fire; FPS stays ≥60 on the Mac. Record min/avg.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: gun params hot-reload + ammo HUD — M1b complete (M1b task 6)"
```

---

## Self-review notes

- Spec coverage (gun slice of the design spec): projectiles as physics events ✓ (T2), payloads/4 ammo ✓ (T3), pixels-as-particles ejecta ✓ (T1/T3), incendiary+spore-detonation chain ✓ (T3 + M1a fire), hot-reload tuning ✓ (T6), aim/fire ✓ (T5).
- Deliberately deferred: growth lifecycle (next plan, M1c), the 1024×2048 world + camera (M1d — M1b plays at 640×384 with a fixed bottom-center muzzle), a real explosion primitive with radial *displacement* (kinetic carve is clear-not-shove for now; radial shove can come with rigid bodies in M2), oil→sludge reflavor.
- Chunk-sleeping guard is tested at every layer (particles/projectiles/payloads each have a "settles to 0" test) — the milestone's non-negotiable invariant.
- Known simplifications, flagged so they aren't mistaken for bugs: kinetic rounds don't crater Rock (keeps generated caves structurally stable without rigid bodies); projectiles fly straight (no gravity) in M1b; the muzzle is a fixed point until there's an avatar.
- Firing is right-click (or the ammo keys select) specifically to avoid colliding with left-drag paint, which stays as the debug tool.
