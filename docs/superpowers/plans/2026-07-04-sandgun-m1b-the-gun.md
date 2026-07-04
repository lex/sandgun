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
    // rock floor at y=60 (Rock is static — a Sand floor would sink and break the test)
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8);
    }
    w.spawn_particle(10.0, 5.0, 0.0, 0.0, Material::Sand as u8);
    assert_eq!(w.particle_count(), 1);
    for _ in 0..400 {
        w.step();
    }
    assert_eq!(w.particle_count(), 0, "particle must resettle, not fly forever");
    // it should have become a grid cell resting on the floor (row 59, just above the rock)
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

### Task 5: Walking avatar — gravity and terrain collision

> **Added 2026-07-04 (user decision):** M1b gets a walking avatar (chosen over the fixed
> muzzle). Scoped tight: an AABB box-collider with gravity, per-axis sub-stepped collision
> against solid/powder terrain, walk + jump. No slopes, no step-up, no sand-pushes-avatar.

**Files:**
- Create: `crates/sandgun-core/src/avatar.rs`
- Modify: `crates/sandgun-core/src/lib.rs` (add `pub mod avatar;`)
- Modify: `crates/sandgun-core/src/world.rs` (add `avatar` field; `spawn_avatar`; `set_avatar_input`; `update_avatar` called in `step`; accessors)
- Create: `crates/sandgun-core/tests/avatar.rs`

**Interfaces:**
- Consumes: `World` grid/`material_at`/`in_bounds`, `Material` class helpers.
- Produces: `avatar::Avatar { x: f32, y: f32, vx: f32, vy: f32, w: i32, h: i32, on_ground: bool, want_left: bool, want_right: bool, want_jump: bool }` (position is the AABB top-left, in world coords); `World.avatar: Option<Avatar>` (pub(crate)); `World::spawn_avatar(x, y)` (w=3, h=6); `World::set_avatar_input(left, right, jump)`; `World::update_avatar()` — applies walk velocity from input, gravity, a jump impulse when `on_ground`, then moves per-axis one pixel at a time, stopping (and zeroing that axis' velocity) when the AABB would overlap a blocking cell (`is_solid() || is_powder()`; liquids/gas passable); sets `on_ground` when a downward move is blocked. Called from `step()` AFTER the cell sweep and projectile/particle updates (so the avatar reacts to the terrain's current state). Accessors: `avatar_xywh() -> Option<[f32;4]>` (x,y,w,h for render + JS muzzle), `avatar_center() -> Option<[f32;2]>`. The avatar reads cells but writes none, so it never adds to `cells_processed` — chunk sleeping is unaffected.

- [ ] **Step 1: Write the failing tests**

`crates/sandgun-core/tests/avatar.rs`:
```rust
use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn avatar_falls_and_rests_on_the_floor() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8); // floor at y=50
    }
    w.spawn_avatar(30.0, 5.0);
    for _ in 0..300 {
        w.step();
    }
    let [_, y, _, h] = w.avatar_xywh().unwrap();
    assert!((y + h - 50.0).abs() <= 1.5, "avatar's feet should rest on the floor (y+h≈50, got {})", y + h);
    let av = w.avatar_center().unwrap();
    assert!(av[1] < 50.0, "avatar is above the floor, not through it");
}

#[test]
fn avatar_is_blocked_by_a_wall() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8); // floor
    }
    for y in 40..50 {
        w.paint(40, y, 0, Material::Rock as u8); // wall at x=40
    }
    w.spawn_avatar(30.0, 44.0);
    w.set_avatar_input(false, true, false); // walk right into the wall
    for _ in 0..300 {
        w.step();
    }
    let [x, _, aw, _] = w.avatar_xywh().unwrap();
    assert!(x + aw <= 40.5, "avatar must not pass through the wall (right edge {} vs wall x=40)", x + aw);
}

#[test]
fn avatar_falls_when_the_ground_is_carved_away() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 40, 0, Material::Rock as u8); // upper floor
        w.paint(x, 60, 0, Material::Rock as u8); // lower floor
    }
    w.spawn_avatar(30.0, 34.0);
    for _ in 0..120 {
        w.step();
    }
    let resting_y = w.avatar_xywh().unwrap()[1];
    // carve the floor out from under it
    for x in 25..40 {
        w.paint(x, 40, 0, Material::Empty as u8);
    }
    for _ in 0..300 {
        w.step();
    }
    let fallen_y = w.avatar_xywh().unwrap()[1];
    assert!(fallen_y > resting_y + 10.0, "avatar must fall after its ground is carved ({resting_y} -> {fallen_y})");
}

#[test]
fn avatar_can_jump_off_the_ground() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8);
    }
    w.spawn_avatar(30.0, 44.0);
    for _ in 0..120 {
        w.step(); // settle onto the floor
    }
    let grounded_y = w.avatar_xywh().unwrap()[1];
    w.set_avatar_input(false, false, true); // jump
    w.step();
    w.set_avatar_input(false, false, false);
    let mut min_y = grounded_y;
    for _ in 0..30 {
        w.step();
        min_y = min_y.min(w.avatar_xywh().unwrap()[1]);
    }
    assert!(min_y < grounded_y - 3.0, "jump must lift the avatar off the floor ({grounded_y} -> {min_y})");
}

#[test]
fn avatar_does_not_add_sim_work_when_resting() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8);
    }
    w.spawn_avatar(30.0, 44.0);
    for _ in 0..300 {
        w.step();
    }
    w.step();
    assert_eq!(w.cells_processed, 0, "a resting avatar writes no cells and keeps the world asleep");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p sandgun-core --test avatar`
Expected: FAIL — `avatar` module / `spawn_avatar` don't exist.

- [ ] **Step 3: Implement the avatar**

`crates/sandgun-core/src/avatar.rs`:
```rust
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
```

`crates/sandgun-core/src/lib.rs`: add `pub mod avatar;`.

In `crates/sandgun-core/src/world.rs`:
- import: `use crate::avatar::Avatar;`
- constants: `const AVATAR_GRAVITY: f32 = 0.3; const AVATAR_WALK: f32 = 1.4; const AVATAR_JUMP: f32 = 4.2; const AVATAR_MAX_FALL: f32 = 6.0;`
- field: `pub(crate) avatar: Option<Avatar>,` init `avatar: None,`.
- methods:
```rust
pub fn spawn_avatar(&mut self, x: f32, y: f32) {
    self.avatar = Some(Avatar {
        x, y, vx: 0.0, vy: 0.0, w: 3, h: 6,
        on_ground: false, want_left: false, want_right: false, want_jump: false,
    });
}

pub fn set_avatar_input(&mut self, left: bool, right: bool, jump: bool) {
    if let Some(a) = self.avatar.as_mut() {
        a.want_left = left;
        a.want_right = right;
        a.want_jump = jump;
    }
}

pub fn avatar_xywh(&self) -> Option<[f32; 4]> {
    self.avatar.map(|a| [a.x, a.y, a.w as f32, a.h as f32])
}

pub fn avatar_center(&self) -> Option<[f32; 2]> {
    self.avatar.map(|a| [a.x + a.w as f32 / 2.0, a.y + a.h as f32 / 2.0])
}

/// Would the AABB with top-left (ax, ay) overlap any blocking cell?
fn avatar_blocked(&self, ax: f32, ay: f32, w: i32, h: i32) -> bool {
    let x0 = ax.floor() as isize;
    let y0 = ay.floor() as isize;
    for dy in 0..h {
        for dx in 0..w {
            let m = self.material_at(x0 + dx as isize, y0 + dy as isize);
            if m.is_solid() || m.is_powder() {
                return true;
            }
        }
    }
    false
}

pub(crate) fn update_avatar(&mut self) {
    let Some(mut a) = self.avatar.take() else { return };
    // horizontal intent
    a.vx = if a.want_left == a.want_right {
        0.0
    } else if a.want_right {
        AVATAR_WALK
    } else {
        -AVATAR_WALK
    };
    // jump
    if a.want_jump && a.on_ground {
        a.vy = -AVATAR_JUMP;
        a.on_ground = false;
    }
    // gravity
    a.vy = (a.vy + AVATAR_GRAVITY).min(AVATAR_MAX_FALL);

    // move X one pixel at a time
    let mut moved = a.vx;
    while moved.abs() >= 0.001 {
        let step = moved.clamp(-1.0, 1.0);
        if self.avatar_blocked(a.x + step, a.y, a.w, a.h) {
            a.vx = 0.0;
            break;
        }
        a.x += step;
        moved -= step;
    }

    // move Y one pixel at a time
    a.on_ground = false;
    let mut moved = a.vy;
    while moved.abs() >= 0.001 {
        let step = moved.clamp(-1.0, 1.0);
        if self.avatar_blocked(a.x, a.y + step, a.w, a.h) {
            if step > 0.0 {
                a.on_ground = true;
            }
            a.vy = 0.0;
            break;
        }
        a.y += step;
        moved -= step;
    }

    // keep in-world horizontally; if it falls out the bottom, leave it (dead-ish) at the edge
    let maxx = (self.width as i32 - a.w) as f32;
    a.x = a.x.clamp(0.0, maxx.max(0.0));
    self.avatar = Some(a);
}
```
- in `step()`, add AFTER `self.update_particles();`: `self.update_avatar();`

- [ ] **Step 4: Run the FULL suite**

Run: `cargo test -p sandgun-core`
Expected: all pass (+5). If `avatar_falls_and_rests_on_the_floor` overshoots, check the per-axis one-pixel sub-stepping stops BEFORE entering the blocked cell (test the tentative position, only commit the step if clear).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: walking avatar — gravity + terrain collision (M1b task 5)"
```

---

### Task 6: WASM avatar surface + aim/fire/move input

**Files:**
- Modify: `crates/sandgun-wasm/src/lib.rs` (avatar passthroughs)
- Create: `web/src/gun.js`
- Modify: `web/src/input.js` (avatar movement keys; expose)
- Modify: `web/src/main.js` (spawn avatar, wire movement + gun, fire from the avatar)

**Interfaces:**
- Consumes: `WasmWorld::fire` (Task 4), the new avatar core API, canvas→world mapping (input.js).
- Produces: `WasmWorld::{spawn_avatar(x,y), set_avatar_input(left,right,jump), avatar_xywh()->Option<Vec<f32>>, avatar_center()->Option<Vec<f32>>}`; `attachGun(canvas, worldW, worldH) -> gun` + `applyGun(gun, world)`. Controls: **A/D or ←/→** walk, **W/↑/Space** jump (avatar); **mouse** aims; **left-click held while over the canvas fires** the gun from the avatar's center toward the cursor — but left-drag currently paints, so gun fire is bound to **left-click** and the debug paintbrush moves to **right-click-drag** (swap the two in input.js; update the README/overlay accordingly in Task 7). Ammo select: `Z/X/C/V` = kinetic/incendiary/acid/spore. Fire rate: one shot per ~6 frames while held. Muzzle = `world.avatar_center()`; if there's no avatar, don't fire. `gun.status` → "gun: incendiary".

Note: `avatar_xywh`/`avatar_center` return `Option<Vec<f32>>` across wasm-bindgen (returns `undefined` in JS when `None`); guard for `undefined` in JS.

- [ ] **Step 1: Implement wasm passthroughs + JS**

Add to `crates/sandgun-wasm/src/lib.rs` inside `impl WasmWorld`:
```rust
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
```

`web/src/gun.js`:
```js
const AMMO = { z: 0, x: 1, c: 2, v: 3 }; // kinetic, incendiary, acid, spore
const AMMO_NAMES = ['kinetic', 'incendiary', 'acid', 'spore'];
const GUN_SPEED = 10;
const FIRE_COOLDOWN = 6; // frames

export function attachGun(canvas, worldW, worldH) {
  const gun = {
    ammo: 0, aimX: worldW / 2, aimY: 0, firing: false, cooldown: 0,
    get status() { return `gun: ${AMMO_NAMES[this.ammo]}`; },
  };
  const toWorld = (e) => {
    const r = canvas.getBoundingClientRect();
    gun.aimX = (e.clientX - r.left) / r.width * worldW;
    gun.aimY = (e.clientY - r.top) / r.height * worldH;
  };
  canvas.addEventListener('mousemove', toWorld);
  canvas.addEventListener('mousedown', (e) => { if (e.button === 0) { gun.firing = true; toWorld(e); } });
  window.addEventListener('mouseup', (e) => { if (e.button === 0) gun.firing = false; });
  window.addEventListener('keydown', (e) => {
    const k = e.key.toLowerCase();
    if (k in AMMO) gun.ammo = AMMO[k];
  });
  return gun;
}

export function applyGun(gun, world) {
  if (gun.cooldown > 0) gun.cooldown--;
  if (!gun.firing || gun.cooldown > 0) return;
  const c = world.avatar_center();
  if (!c) return; // no avatar, no muzzle
  const [mx, my] = c;
  const dx = gun.aimX - mx;
  const dy = gun.aimY - my;
  const len = Math.hypot(dx, dy) || 1;
  world.fire(mx, my, (dx / len) * GUN_SPEED, (dy / len) * GUN_SPEED, gun.ammo);
  gun.cooldown = FIRE_COOLDOWN;
}
```

In `web/src/input.js`, add avatar-movement tracking to the `input` object (a `move` set): track W/A/S/D + arrows + Space as held keys, and swap the paint button to **right-click** (so left-click is free for the gun). Add to the input object: `left:false, right:false, jump:false`, update them on keydown/keyup for `a`/`arrowleft`, `d`/`arrowright`, `w`/`arrowup`/` `. Change the paint pointerdown guard to fire only on `e.button === 2` (right button), and add `canvas.addEventListener('contextmenu', e => e.preventDefault())`.

In `web/src/main.js`:
```js
import { attachGun, applyGun } from './gun.js';
// after world.generate(...):
world.spawn_avatar(W / 2, 4);
// after attachInput:
const gun = attachGun(document.getElementById('view'), W, H);
// inside frame(), before world.step():
world.set_avatar_input(input.left, input.right, input.jump);
applyGun(gun, world);
```

- [ ] **Step 2: Verify in the browser (Playwright)**

`./scripts/build-wasm.sh`, dev server. Expected: a small avatar spawns and falls onto the terrain; A/D walk it, W/Space jumps; it's stopped by walls and rides on top of dirt/mushrooms; left-click fires the selected ammo from the avatar toward the cursor (tracer flies, craters/fire/acid/mycelium land on impact); if you carve the ground from under it (fire an acid/kinetic round at its feet) it falls. Right-drag still paints (debug). No console errors.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: avatar wasm surface + move/aim/fire input (M1b task 6)"
```

---

### Task 7: Avatar render + gun params hot-reload + M1b acceptance

**Files:**
- Modify: `crates/sandgun-core/src/world.rs` (stamp the avatar in `render_rgba`)
- Modify: `web/public/params.json`, `web/src/params.js` (5 payload params)
- Modify: `web/src/overlay.js` + `web/src/main.js` (HUD shows ammo)
- Modify: `README.md` (update controls: gun on left-click, paint on right-click, avatar keys)

**Interfaces:**
- Consumes: everything above.
- Produces: the avatar drawn as a distinct sprite-ish block in the buffer; payload params hot-reloadable (`P`); ammo in the HUD; README current; M1b accepted.

- [ ] **Step 1: Render the avatar**

At the end of `render_rgba` in `crates/sandgun-core/src/world.rs` (after particles/projectiles), add:
```rust
    if let Some(a) = self.avatar {
        let (x0, y0) = (a.x.floor() as isize, a.y.floor() as isize);
        for dy in 0..a.h {
            for dx in 0..a.w {
                let (cx, cy) = (x0 + dx as isize, y0 + dy as isize);
                if self.in_bounds(cx, cy) {
                    let o = (cy as usize * self.width + cx as usize) * 4;
                    self.rgba[o] = 90;
                    self.rgba[o + 1] = 220;
                    self.rgba[o + 2] = 240;
                    self.rgba[o + 3] = 255;
                }
            }
        }
    }
```

- [ ] **Step 2: Extend the params pipeline**

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
In `web/src/overlay.js`, append `· ${gun.status}` to the HUD line (pass `gun` into `drawOverlay` from `main.js`). Update `README.md`'s controls: **left-click** fires, **right-drag** paints (debug), **A/D/←/→** walk, **W/Space** jump, **Z/X/C/V** ammo.

- [ ] **Step 3: Rebuild + browser acceptance (the grin test)**

`./scripts/build-wasm.sh`, dev server. Run the M1b acceptance with Playwright + eyeballs:
- Avatar spawns, falls onto terrain, walks (A/D) and jumps (W) with wall/floor collision.
- Fire each ammo from the avatar; confirm the four distinct terrain-verbs (crater+debris / fire+fuses / acid melt / mycelium plant).
- **Chain-reaction beat:** incendiary into a spore pocket or along a mycelium vein cascades (fuse burns, spore gas detonates) — the core fantasy.
- **Self-endangerment beat:** shooting the ground beneath the avatar drops it — the terrain and the character share one world.
- Tune `params.json` (bump `kinetic_radius`), press `P`, bigger craters, no rebuild.
- **Settling:** after a burst, stop; debug `D` shows chunk boxes clear and `cells_processed` → 0 (entities + avatar don't leak activity).
- **Perf:** sustained fire + movement; FPS ≥60 on the Mac. Record min/avg.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: avatar render + gun params + ammo HUD — M1b complete (M1b task 7)"
```

---

## Self-review notes

- Spec coverage (gun slice of the design spec): projectiles as physics events ✓ (T2), payloads/4 ammo ✓ (T3), pixels-as-particles ejecta ✓ (T1/T3), incendiary+spore-detonation chain ✓ (T3 + M1a fire), walking avatar w/ jump + terrain collision ✓ (T5), aim/fire from the avatar ✓ (T6), avatar render + hot-reload tuning ✓ (T7).
- Deliberately deferred: growth lifecycle (next plan, M1c), the 1024×2048 world + camera (M1d — M1b plays at 640×384 with the avatar on-screen), a real explosion primitive with radial *displacement* (kinetic carve is clear-not-shove for now; radial shove can come with rigid bodies in M2), oil→sludge reflavor.
- Chunk-sleeping guard is tested at every layer (particles/projectiles/payloads settle to 0; the avatar writes no cells) — the milestone's non-negotiable invariant.
- Known simplifications, flagged so they aren't mistaken for bugs: kinetic rounds don't crater Rock (keeps generated caves structurally stable without rigid bodies); projectiles fly straight (no gravity) in M1b; the avatar is a plain AABB — no slopes, no step-up, terrain (falling sand) doesn't push it, it only collides against settled cells, and sand can visually overlap it.
- Control split: **left-click fires**, **right-drag paints** (debug), so they don't collide. Avatar walks on A/D/←/→ and jumps on W/Space.
