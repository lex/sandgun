# SANDGUN M1c — "The World Grows Back" Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The fungal cavern comes alive — mycelium creeps across soil and bridges the craters you shoot, mature patches fruit parametric mushrooms cell-by-cell, caps puff spores that reseed new colonies — all on a budgeted growth frontier so the sleeping-world optimization survives.

**Architecture:** A new `sandgun-core::growth` module adds a **budgeted frontier**: `World` keeps a bounded `Vec<FrontierCell>` and a `Vec<GrowingMushroom>`. A new `World::grow()` runs on an N-frame cadence *inside* `step()` (after the cell sweep, before entity updates); it processes a budgeted slice — each frontier cell colonizes one eligible neighbor (soil, or empty within reach), new growth joins the frontier, exhausted cells retire. Growth mutates cells and `wake()`s their chunks so the falling-sand sim reacts; when the frontier and mushroom lists are empty, `grow()` is a no-op and the world sleeps normally. The falling-sand `step()` sweep is otherwise untouched. Design spec: `docs/superpowers/specs/2026-07-04-m1c-growth-design.md`.

**Tech Stack:** unchanged (Rust stable, wasm-bindgen/wasm-pack via `scripts/build-wasm.sh`, Vite, WebGL2).

## Global Constraints

- Cell stays 4 bytes; flags bit 7 reserved (M2 rigid bodies); `FLAG_BURNING = 0b10`. NO per-cell temperature.
- **Maturity lives in `aux`** on non-burning mycelium (age counter, saturating). `aux` only means "fuel" once `FLAG_BURNING` is set, and burning mycelium is dying — no conflict. **Reach-from-soil lives on the `FrontierCell` entry, never in `aux`** — the two counters never share the byte.
- **Chunk sleeping is sacred.** Every growth cell mutation calls `wake()`. `grow()` early-returns when both the frontier and mushroom lists are empty (zero cost on a settled world). The frontier MUST retire exhausted cells so it can drain to empty — growth must be able to reach a terminal state. The kill-criterion regression test asserts: after growth settles and the frontier empties, `cells_processed` returns to 0.
- Determinism: ALL growth randomness via `World::next_rand`/`World::chance`. No wall-clock, no `Math.random` in the sim.
- Sim logic (frontier, growth, mushrooms) lives only in `sandgun-core`. `sandgun-wasm` stays glue-only (one-line passthroughs). Params glue + HUD in `web/`.
- Material ids unchanged: `Empty=0 Rock=1 Sand=2 Water=3 Oil=4 Soil=5 Mycelium=6 MushroomFlesh=7 SporeGas=8 Smoke=9 Ash=10 Acid=11 Fire=12`.
- Coordinates: cell grid is integer, `usize`/`isize`. `in_bounds(x: isize, y: isize)`, `material_at(x: isize, y: isize) -> Material`, `idx(x,y)`, `get(x,y)`, `wake(x,y)`, `next_rand() -> u32`, `chance(p: f32) -> bool`, `cell_flags(x,y)`, `cell_aux(x,y)` all already exist on `World`. `+y` is down.
- Growth runs in the **current 640×384 world** (big world + follow-cam is M1d).
- Params: `params.rs` index constants are mirrored in `web/src/params.js` + `web/public/params.json` — **keep all three in sync within the same task that adds a param** (a Rust/JS param desync was a finding in M1b).
- All commits end with: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` (second `-m`). Run cargo from repo root. Rebuild wasm with `./scripts/build-wasm.sh`.
- Work on a branch: `git checkout -b m1c-growth` before Task 1.

## File Structure

- **Create** `crates/sandgun-core/src/growth.rs` — `FrontierCell`, `GrowingMushroom` structs AND the `impl World` growth methods (`seed_frontier`, `grow`, `colonize_from`, `try_fruit`, `grow_mushrooms`, `puff_and_reseed`). Kept out of `world.rs` because that file is already ~950 lines; same struct-in-own-module pattern as `particle.rs`/`projectile.rs`, but the `impl World` block lives here too to keep `world.rs` lean.
- **Create** `crates/sandgun-core/tests/growth.rs` — all M1c core tests.
- **Modify** `crates/sandgun-core/src/lib.rs` — `pub mod growth;`
- **Modify** `crates/sandgun-core/src/world.rs` — new `World` fields (frontier, mushrooms, growth counters), init in `new()`, `grow()` cadence hook in `step()`, seed call, count accessors, mushroom-cap release in the impact path (Task 6).
- **Modify** `crates/sandgun-core/src/params.rs` — growth param indices + defaults.
- **Modify** `crates/sandgun-wasm/src/lib.rs` — `frontier_count`/`mushroom_count` passthroughs (Task 7).
- **Modify** `web/public/params.json`, `web/src/params.js` — growth params mirror (Task 1).
- **Modify** `web/src/overlay.js` — show frontier/mushroom counts in debug HUD (Task 7).

---

### Task 1: Frontier scaffold + colonize soil + growth cadence

**Files:**
- Create: `crates/sandgun-core/src/growth.rs`
- Create: `crates/sandgun-core/tests/growth.rs`
- Modify: `crates/sandgun-core/src/lib.rs`
- Modify: `crates/sandgun-core/src/world.rs`
- Modify: `crates/sandgun-core/src/params.rs`
- Modify: `web/public/params.json`, `web/src/params.js`

**Interfaces:**
- Consumes: `World` internals (`cells`, `idx`, `in_bounds`, `material_at`, `wake`, `next_rand`, `chance`, `params`), `Material`, `Cell`, `FLAG_BURNING`.
- Produces: `FrontierCell { x: usize, y: usize, reach: u8 }`; `GrowingMushroom` (fields defined here, populated in Task 4 — for now `{ x: usize, base_y: usize, height: u8, cap_r: u8, progress: u16 }`); `World.frontier: Vec<FrontierCell>`, `World.mushrooms: Vec<GrowingMushroom>`, `World.grow_countdown: u32`; `World::seed_frontier()`, `World::grow()`, `World::colonize_from(i) -> bool`, `World::frontier_len() -> usize`, `World::mushroom_len() -> usize`. All growth params: `P_GROWTH_INTERVAL, P_GROWTH_BUDGET, P_MAX_FRONTIER, P_MAX_REACH, P_WATER_ACCEL, P_MATURITY, P_MAX_MUSHROOMS, P_FRUIT_CHANCE, P_MUSH_HEIGHT_MIN, P_MUSH_HEIGHT_MAX, P_MUSH_CAP_MIN, P_MUSH_CAP_MAX, P_MUSH_REVEAL, P_PUFF_INTERVAL, P_PUFF_SPORES, P_RESEED_CHANCE` and new `P_COUNT`.

- [ ] **Step 1: Add growth params (Rust + JS mirror, all at once)**

In `crates/sandgun-core/src/params.rs`, replace the `P_SPORE_BLOB_RADIUS`/`P_COUNT` lines and extend `Default`:
```rust
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
pub const P_COUNT: usize = 35;
```
Append these to the `Default` `v[...] = ...` block:
```rust
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
```
In `web/public/params.json`, add before the closing brace (comma after `spore_blob_radius`):
```json
  "spore_blob_radius": 4,
  "growth_interval": 3,
  "growth_budget": 24,
  "max_frontier": 4096,
  "max_reach": 4,
  "water_accel": 2,
  "maturity": 90,
  "max_mushrooms": 6,
  "fruit_chance": 0.02,
  "mush_height_min": 6,
  "mush_height_max": 16,
  "mush_cap_min": 3,
  "mush_cap_max": 7,
  "mush_reveal": 2,
  "puff_interval": 120,
  "puff_spores": 5,
  "reseed_chance": 0.1
```
In `web/src/params.js`, extend the `INDEX` map:
```js
  acid_blob_radius: 17, spore_blob_radius: 18,
  growth_interval: 19, growth_budget: 20, max_frontier: 21, max_reach: 22,
  water_accel: 23, maturity: 24, max_mushrooms: 25, fruit_chance: 26,
  mush_height_min: 27, mush_height_max: 28, mush_cap_min: 29, mush_cap_max: 30,
  mush_reveal: 31, puff_interval: 32, puff_spores: 33, reseed_chance: 34,
```

- [ ] **Step 2: Write the failing tests**

Create `crates/sandgun-core/tests/growth.rs`:
```rust
use sandgun_core::cell::Material;
use sandgun_core::world::World;

// A world with a solid rock floor and a band of soil above it, no worldgen.
fn soil_world() -> World {
    let mut w = World::new(128, 128);
    for x in 0..128 {
        w.paint(x as i32, 100, 0, Material::Rock as u8);
        for y in 80..100 {
            w.paint(x as i32, y as i32, 0, Material::Soil as u8);
        }
    }
    w
}

#[test]
fn empty_frontier_grow_is_noop() {
    let mut w = soil_world();
    // no mycelium anywhere -> nothing to seed, grow() must do nothing
    w.seed_frontier();
    assert_eq!(w.frontier_len(), 0);
    for _ in 0..50 {
        w.step();
    }
    // settled world with an empty frontier costs nothing
    w.step();
    assert_eq!(w.cells_processed, 0);
}

#[test]
fn mycelium_colonizes_adjacent_soil_over_ticks() {
    let mut w = soil_world();
    w.paint(64, 90, 0, Material::Mycelium as u8); // one seed in the soil band
    w.seed_frontier();
    assert!(w.frontier_len() >= 1, "seed should enter the frontier");
    let before = mycelium_count(&w);
    for _ in 0..120 {
        w.step();
    }
    let after = mycelium_count(&w);
    assert!(after > before + 5, "mycelium should spread into soil ({before} -> {after})");
}

#[test]
fn growth_settles_and_frontier_drains() {
    // small fully-enclosed soil pocket: growth must terminate and the world sleep
    let mut w = World::new(64, 64);
    for x in 20..30 {
        for y in 20..30 {
            w.paint(x, y, 0, Material::Soil as u8);
        }
    }
    w.paint(25, 25, 0, Material::Mycelium as u8);
    w.seed_frontier();
    for _ in 0..2000 {
        w.step();
    }
    assert_eq!(w.frontier_len(), 0, "frontier must retire to empty");
    w.step();
    assert_eq!(w.cells_processed, 0, "settled grown world must sleep");
}

fn mycelium_count(w: &World) -> usize {
    let mut n = 0;
    for y in 0..w.height {
        for x in 0..w.width {
            if w.get(x, y) == Material::Mycelium {
                n += 1;
            }
        }
    }
    n
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p sandgun-core --test growth`
Expected: FAIL — `seed_frontier`/`frontier_len`/`grow` don't exist.

- [ ] **Step 4: Implement the module and wire it in**

`crates/sandgun-core/src/lib.rs` — add near the other `pub mod` lines:
```rust
pub mod growth;
```

Create `crates/sandgun-core/src/growth.rs`:
```rust
use crate::cell::{Material, FLAG_BURNING};
use crate::params::*;
use crate::world::World;

/// A cell on the growing edge of the mycelium network.
#[derive(Clone, Copy)]
pub struct FrontierCell {
    pub x: usize,
    pub y: usize,
    /// How many empty cells this growth is from solid soil/mycelium mass.
    /// 0 = grew from/into soil; increments when bridging empty. Lives here, never in `aux`.
    pub reach: u8,
}

/// A mushroom being revealed cell-by-cell. Shape fields set at fruiting (Task 4).
#[derive(Clone, Copy)]
pub struct GrowingMushroom {
    pub x: usize,
    pub base_y: usize,
    pub height: u8,
    pub cap_r: u8,
    pub progress: u16,
}

impl World {
    /// Scan once (at load / after worldgen) and enqueue mycelium cells that border
    /// colonizable space. O(width*height) but runs only on demand, not per frame.
    pub fn seed_frontier(&mut self) {
        self.frontier.clear();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) == Material::Mycelium && self.has_colonizable_neighbor(x, y) {
                    self.frontier.push(FrontierCell { x, y, reach: 0 });
                }
            }
        }
    }

    fn has_colonizable_neighbor(&self, x: usize, y: usize) -> bool {
        for (nx, ny) in self.ortho(x, y) {
            if self.material_at(nx, ny) == Material::Soil {
                return true;
            }
        }
        false
    }

    /// The 4 orthogonal neighbors as isize pairs (may be out of bounds).
    fn ortho(&self, x: usize, y: usize) -> [(isize, isize); 4] {
        let (xi, yi) = (x as isize, y as isize);
        [(xi + 1, yi), (xi - 1, yi), (xi, yi + 1), (xi, yi - 1)]
    }

    pub fn frontier_len(&self) -> usize {
        self.frontier.len()
    }
    pub fn mushroom_len(&self) -> usize {
        self.mushrooms.len()
    }

    /// Budgeted growth tick. Called from step() on the P_GROWTH_INTERVAL cadence.
    /// Returns immediately when there is nothing alive to grow (chunk-sleep safe).
    pub fn grow(&mut self) {
        if self.frontier.is_empty() && self.mushrooms.is_empty() {
            return;
        }
        let budget = self.params.values[P_GROWTH_BUDGET] as usize;
        let max_frontier = self.params.values[P_MAX_FRONTIER] as usize;

        // Process a budgeted slice of the frontier. Swap-remove retirees; append new growth.
        let mut processed = 0;
        let mut i = 0;
        while i < self.frontier.len() && processed < budget {
            processed += 1;
            let fc = self.frontier[i];
            // Retire if the source cell is no longer living mycelium (shot, burned, etc).
            if self.get(fc.x, fc.y) != Material::Mycelium {
                self.frontier.swap_remove(i);
                continue;
            }
            let grew = self.colonize_from(i);
            if !self.has_colonizable_neighbor_or_bridge(fc.x, fc.y, fc.reach) {
                self.frontier.swap_remove(i); // exhausted -> retire
            } else {
                i += 1;
            }
            let _ = grew;
            if self.frontier.len() > max_frontier {
                self.frontier.truncate(max_frontier); // hard cap; see Task 7 note re: logging drops
            }
        }
        // Mushroom growth + puffs are added in Tasks 3-5; for now a no-op if empty.
    }

    /// Try to colonize one neighbor of frontier cell `i`. Returns true if a cell was converted.
    /// Task 1: soil only. Task 2 extends this to bridge empty within reach.
    pub fn colonize_from(&mut self, i: usize) -> bool {
        let fc = self.frontier[i];
        // Randomize neighbor order so growth isn't directionally biased.
        let order = self.shuffled_ortho(fc.x, fc.y);
        for (nx, ny) in order {
            if self.material_at(nx, ny) == Material::Soil {
                let (ux, uy) = (nx as usize, ny as usize);
                self.set_mycelium(ux, uy);
                self.frontier.push(FrontierCell { x: ux, y: uy, reach: 0 });
                return true;
            }
        }
        false
    }

    /// Place a fresh mycelium cell: material + reset aux age to 0 + wake its chunk.
    pub(crate) fn set_mycelium(&mut self, x: usize, y: usize) {
        let idx = self.idx(x, y);
        self.cells[idx].material = Material::Mycelium as u8;
        self.cells[idx].aux = 0; // age starts at 0 (maturity clock, Task 3)
        self.cells[idx].flags &= !FLAG_BURNING;
        self.wake(x, y);
    }

    fn has_colonizable_neighbor_or_bridge(&self, x: usize, y: usize, _reach: u8) -> bool {
        // Task 1: soil only. Task 2 adds the in-reach empty check.
        self.has_colonizable_neighbor(x, y)
    }

    fn shuffled_ortho(&mut self, x: usize, y: usize) -> [(isize, isize); 4] {
        let mut a = self.ortho(x, y);
        // Fisher-Yates with the sim RNG (deterministic).
        for k in (1..4).rev() {
            let j = (self.next_rand() as usize) % (k + 1);
            a.swap(k, j);
        }
        a
    }
}
```

In `crates/sandgun-core/src/world.rs`:
- Add `use crate::growth::{FrontierCell, GrowingMushroom};` near the top imports.
- Add fields to `struct World` (after the entity fields like `particles`/`projectiles`):
```rust
    pub(crate) frontier: Vec<FrontierCell>,
    pub(crate) mushrooms: Vec<GrowingMushroom>,
    pub(crate) grow_countdown: u32,
```
- Initialize them in `World::new` (in the struct literal):
```rust
            frontier: Vec::new(),
            mushrooms: Vec::new(),
            grow_countdown: 0,
```
- In `step()`, after the cell-sweep double-loop and BEFORE `self.update_projectiles();`, add the cadence hook:
```rust
        // Budgeted growth on the P_GROWTH_INTERVAL cadence (chunk-sleep safe: grow() no-ops when idle).
        if self.grow_countdown == 0 {
            self.grow();
            self.grow_countdown = (self.params.values[crate::params::P_GROWTH_INTERVAL] as u32).max(1);
        }
        self.grow_countdown -= 1;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p sandgun-core --test growth`
Expected: 3 pass. Then full `cargo test -p sandgun-core` — all prior tests still pass (65 + 3).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: budgeted growth frontier — mycelium colonizes soil (M1c task 1)"
```

---

### Task 2: Bridge empty (range-limited) + water acceleration

**Files:**
- Modify: `crates/sandgun-core/src/growth.rs` (extend `colonize_from`, `has_colonizable_neighbor_or_bridge`, add water accel)
- Modify: `crates/sandgun-core/tests/growth.rs`

**Interfaces:**
- Consumes: `P_MAX_REACH`, `P_WATER_ACCEL`, the frontier from Task 1.
- Produces: mycelium that bridges `Empty` within `reach < P_MAX_REACH` of soil/mycelium mass (reach carried on the new `FrontierCell`); water-adjacent frontier cells colonize extra times per tick.

- [ ] **Step 1: Write the failing tests**

Append to `crates/sandgun-core/tests/growth.rs`:
```rust
#[test]
fn mycelium_bridges_a_small_gap_but_not_open_air() {
    // soil platform with a 3-wide empty notch carved in it; mycelium should bridge the notch
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 40, 0, Material::Soil as u8);
    }
    for x in 30..33 {
        w.paint(x, 40, 0, Material::Empty as u8); // the notch
    }
    w.paint(20, 40, 0, Material::Mycelium as u8);
    w.seed_frontier();
    for _ in 0..600 {
        w.step();
    }
    // it reached across the notch (a cell in the former gap is now mycelium)
    let bridged = (30..33).any(|x| w.get(x, 40) == Material::Mycelium);
    assert!(bridged, "mycelium should bridge the small empty notch");
    // but it did NOT grow far up into open air above the platform
    assert_eq!(w.get(20, 30), Material::Empty, "must not sprawl into open air");
}

#[test]
fn bridging_respects_max_reach() {
    // mycelium on a single soil pillar with open air beside it, max_reach small
    let mut w = World::new(64, 64);
    w.set_param(sandgun_core::params::P_MAX_REACH as u32, 2.0);
    for y in 30..50 {
        w.paint(10, y, 0, Material::Soil as u8); // a vertical soil pillar
    }
    w.paint(10, 40, 0, Material::Mycelium as u8);
    w.seed_frontier();
    for _ in 0..800 {
        w.step();
    }
    // mycelium may reach at most 2 empty cells right of the pillar (x=11,12), never x=14
    assert_eq!(w.get(14, 40), Material::Empty, "reach cap must stop bridging at 2 cells");
}
```
(`set_param` already exists on `World` via the wasm crate? No — it's on `WasmWorld`. Add a native `World::set_param` if missing — check `world.rs`; if absent, add:
```rust
    pub fn set_param(&mut self, index: usize, value: f32) {
        if index < crate::params::P_COUNT {
            self.params.values[index] = value;
        }
    }
```
and have `WasmWorld::set_param` delegate to it. If `World::set_param` already exists, skip.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p sandgun-core --test growth`
Expected: the two new tests FAIL (no bridging yet); Task 1 tests still pass.

- [ ] **Step 3: Extend colonize to bridge empty within reach + water accel**

Replace `colonize_from` and `has_colonizable_neighbor_or_bridge` in `growth.rs`:
```rust
    pub fn colonize_from(&mut self, i: usize) -> bool {
        let fc = self.frontier[i];
        let max_reach = self.params.values[P_MAX_REACH] as u8;
        // Water contact accelerates: one base attempt + extra attempts when a neighbor is water.
        let attempts = if self.water_adjacent(fc.x, fc.y) {
            1 + self.params.values[P_WATER_ACCEL] as u32
        } else {
            1
        };
        let mut grew = false;
        for _ in 0..attempts {
            let order = self.shuffled_ortho(fc.x, fc.y);
            let mut did = false;
            for (nx, ny) in order {
                let m = self.material_at(nx, ny);
                if m == Material::Soil {
                    let (ux, uy) = (nx as usize, ny as usize);
                    self.set_mycelium(ux, uy);
                    self.frontier.push(FrontierCell { x: ux, y: uy, reach: 0 });
                    did = true;
                    break;
                }
                // Bridge into empty only if this growth is still within reach of soil mass.
                if m == Material::Empty && fc.reach < max_reach && self.in_bounds(nx, ny) {
                    let (ux, uy) = (nx as usize, ny as usize);
                    self.set_mycelium(ux, uy);
                    self.frontier.push(FrontierCell { x: ux, y: uy, reach: fc.reach + 1 });
                    did = true;
                    break;
                }
            }
            grew |= did;
            if !did {
                break;
            }
        }
        grew
    }

    fn has_colonizable_neighbor_or_bridge(&self, x: usize, y: usize, reach: u8) -> bool {
        let max_reach = self.params.values[P_MAX_REACH] as u8;
        for (nx, ny) in self.ortho(x, y) {
            let m = self.material_at(nx, ny);
            if m == Material::Soil {
                return true;
            }
            if m == Material::Empty && reach < max_reach && self.in_bounds(nx, ny) {
                return true;
            }
        }
        false
    }

    fn water_adjacent(&self, x: usize, y: usize) -> bool {
        self.ortho(x, y).iter().any(|&(nx, ny)| self.material_at(nx, ny) == Material::Water)
    }
```
Note: `material_at` returns `Rock` for out-of-bounds, so the `in_bounds` guard on the empty branch prevents bridging off the world edge.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sandgun-core --test growth` then `cargo test -p sandgun-core`
Expected: all pass. If `bridging_respects_max_reach` still fails, the `reach` isn't being carried onto the new `FrontierCell` — verify `fc.reach + 1`.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: mycelium bridges craters within reach; water accelerates (M1c task 2)"
```

---

### Task 3: Maturity aging + fruiting trigger

**Files:**
- Modify: `crates/sandgun-core/src/growth.rs` (age mycelium; enqueue mushrooms under a cap)
- Modify: `crates/sandgun-core/tests/growth.rs`

**Interfaces:**
- Consumes: `P_MATURITY`, `P_MAX_MUSHROOMS`, `P_FRUIT_CHANCE`, `P_MUSH_HEIGHT_MIN/MAX`, `P_MUSH_CAP_MIN/MAX`.
- Produces: `aux` on non-burning mycelium increments per growth tick (saturating at 255); a frontier cell whose `aux >= P_MATURITY`, when `mushrooms.len() < P_MAX_MUSHROOMS`, has a `P_FRUIT_CHANCE` chance per growth tick to push a `GrowingMushroom` (shape rolled from params via `next_rand`). `World::try_fruit(x, y) -> bool`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/sandgun-core/tests/growth.rs`:
```rust
#[test]
fn undisturbed_mycelium_ages_toward_maturity() {
    let mut w = soil_world();
    w.paint(64, 90, 0, Material::Mycelium as u8);
    w.seed_frontier();
    for _ in 0..300 {
        w.step();
    }
    // the original seed has been alive the whole time -> its aux age climbed
    assert!(w.cell_aux(64, 90) >= 90, "mature mycelium should have high aux age");
}

#[test]
fn mature_mycelium_fruits_a_mushroom_under_the_cap() {
    let mut w = soil_world();
    w.set_param(sandgun_core::params::P_FRUIT_CHANCE as u32, 1.0); // force fruiting when eligible
    w.set_param(sandgun_core::params::P_MATURITY as u32, 10.0);
    w.paint(64, 90, 0, Material::Mycelium as u8);
    w.seed_frontier();
    for _ in 0..200 {
        w.step();
    }
    assert!(w.mushroom_len() >= 1, "a mature patch should fruit");
    assert!(w.mushroom_len() <= 6, "must respect the global mushroom cap (default 6)");
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p sandgun-core --test growth`
Expected: both new tests FAIL.

- [ ] **Step 3: Implement aging + fruiting**

In `growth.rs`, at the top of the frontier loop body in `grow()` (right after `let fc = self.frontier[i];` and the "still mycelium?" retire check), age the cell and maybe fruit:
```rust
            // Age the living cell toward fruiting maturity (aux is free until it burns).
            let ci = self.idx(fc.x, fc.y);
            if self.cells[ci].flags & FLAG_BURNING == 0 {
                self.cells[ci].aux = self.cells[ci].aux.saturating_add(1);
            }
            // Fruit: mature, under the global cap, on a die roll.
            let maturity = self.params.values[P_MATURITY] as u8;
            let cap = self.params.values[P_MAX_MUSHROOMS] as usize;
            if self.cells[ci].aux >= maturity
                && self.mushrooms.len() < cap
                && self.chance(self.params.values[P_FRUIT_CHANCE])
            {
                self.try_fruit(fc.x, fc.y);
            }
```
Add the `try_fruit` method:
```rust
    /// Roll a parametric mushroom shape and enqueue it to grow from (x, y).
    pub fn try_fruit(&mut self, x: usize, y: usize) -> bool {
        let hmin = self.params.values[P_MUSH_HEIGHT_MIN] as i32;
        let hmax = self.params.values[P_MUSH_HEIGHT_MAX] as i32;
        let cmin = self.params.values[P_MUSH_CAP_MIN] as i32;
        let cmax = self.params.values[P_MUSH_CAP_MAX] as i32;
        let height = self.rand_range(hmin, hmax) as u8;
        let cap_r = self.rand_range(cmin, cmax) as u8;
        self.mushrooms.push(GrowingMushroom { x, base_y: y, height, cap_r, progress: 0 });
        true
    }

    /// Inclusive random integer in [lo, hi] using the sim RNG. lo<=hi assumed; falls back to lo.
    pub(crate) fn rand_range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next_rand() as i32).rem_euclid(hi - lo + 1)
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p sandgun-core --test growth` then `cargo test -p sandgun-core`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: mycelium matures in aux and fruits mushrooms under a cap (M1c task 3)"
```

---

### Task 4: Parametric mushroom growth (cell-by-cell reveal)

**Files:**
- Modify: `crates/sandgun-core/src/growth.rs` (grow mushrooms; call from `grow()`)
- Modify: `crates/sandgun-core/tests/growth.rs`

**Interfaces:**
- Consumes: `P_MUSH_REVEAL`, the `mushrooms` list, `GrowingMushroom` shape fields.
- Produces: `World::grow_mushrooms()` — each growth tick, each mushroom reveals up to `P_MUSH_REVEAL` more `MushroomFlesh` cells: stem column (bottom-up from `base_y-1`) for `height` cells, then a filled cap disk of radius `cap_r` centered above the stem top. `progress` tracks revealed cells; a mushroom whose `progress` covers stem+cap retires from the list. Each write `wake()`s its chunk and only overwrites `Empty`/`Soil` (won't erase rock/water).

- [ ] **Step 1: Write the failing tests**

Append to `crates/sandgun-core/tests/growth.rs`:
```rust
#[test]
fn a_mushroom_grows_stem_then_cap_and_retires() {
    let mut w = World::new(64, 64);
    // open space above a floor so the mushroom has room
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8);
    }
    // directly enqueue a mushroom (bypass fruiting RNG) via the public try_fruit
    w.try_fruit(32, 49);
    let steps_needed = 2000;
    let mut saw_stem = false;
    for _ in 0..steps_needed {
        w.step();
        if w.get(32, 45) == Material::MushroomFlesh {
            saw_stem = true;
        }
        if w.mushroom_len() == 0 {
            break;
        }
    }
    assert!(saw_stem, "stem should have been revealed above the base");
    assert_eq!(w.mushroom_len(), 0, "completed mushroom must retire from the list");
    // a cap cell to the side of the stem top exists (cap is wider than the stem)
    let cap_present = (28..37).any(|x| {
        (30..45).any(|y| w.get(x, y) == Material::MushroomFlesh)
    });
    assert!(cap_present, "cap disk should have been revealed");
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p sandgun-core --test growth`
Expected: FAIL — mushrooms never reveal (list drains without drawing) or `grow_mushrooms` missing.

- [ ] **Step 3: Implement mushroom reveal**

In `grow()`, after the frontier loop, add:
```rust
        self.grow_mushrooms();
```
Add to `growth.rs`:
```rust
    /// Reveal more of each growing mushroom this tick; retire finished ones.
    pub fn grow_mushrooms(&mut self) {
        let reveal = self.params.values[P_MUSH_REVEAL] as u16;
        let mut i = 0;
        while i < self.mushrooms.len() {
            let done = self.reveal_mushroom(i, reveal);
            if done {
                self.mushrooms.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Reveal up to `n` cells of mushroom `i`. Returns true when fully grown.
    /// Layout: cells [0, height) are the stem column going up from base_y-1;
    /// cells [height, height + cap_area) are the cap disk around the stem top.
    fn reveal_mushroom(&mut self, i: usize, n: u16) -> bool {
        let m = self.mushrooms[i];
        let stem = m.height as u16;
        let r = m.cap_r as i32;
        let cap_top_y = m.base_y as i32 - m.height as i32; // center of the cap
        // Precompute the cap disk offsets in a stable order (top-down, left-right) for determinism.
        let cap_cells = cap_disk(r); // Vec<(dx, dy)>
        let total = stem + cap_cells.len() as u16;

        let mut revealed = 0;
        while revealed < n && m.progress + revealed < total {
            let p = m.progress + revealed;
            let (cx, cy) = if p < stem {
                (m.x as i32, m.base_y as i32 - 1 - p as i32) // stem, bottom-up
            } else {
                let (dx, dy) = cap_cells[(p - stem) as usize];
                (m.x as i32 + dx, cap_top_y + dy)
            };
            if self.in_bounds(cx, cy) {
                let cur = self.material_at(cx, cy);
                if cur == Material::Empty || cur == Material::Soil {
                    let idx = self.idx(cx as usize, cy as usize);
                    self.cells[idx].material = Material::MushroomFlesh as u8;
                    self.cells[idx].aux = 0;
                    self.wake(cx as usize, cy as usize);
                }
            }
            revealed += 1;
        }
        self.mushrooms[i].progress += revealed;
        self.mushrooms[i].progress >= total
    }
```
Add this free function at the bottom of `growth.rs` (outside `impl World`):
```rust
/// Filled disk of radius r as (dx, dy) offsets, deterministic order (row-major top-down).
fn cap_disk(r: i32) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                cells.push((dx, dy));
            }
        }
    }
    cells
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p sandgun-core --test growth` then `cargo test -p sandgun-core`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: parametric mushrooms grow stem-then-cap cell by cell (M1c task 4)"
```

---

### Task 5: Puff + reseed

**Files:**
- Modify: `crates/sandgun-core/src/growth.rs` (cap puffs; spores reseed soil)
- Modify: `crates/sandgun-core/src/world.rs` (puff cadence counter field if needed)
- Modify: `crates/sandgun-core/tests/growth.rs`

**Interfaces:**
- Consumes: `P_PUFF_INTERVAL`, `P_PUFF_SPORES`, `P_RESEED_CHANCE`; the `SporeGas` material.
- Produces: completed mushroom caps (tracked as `MushroomFlesh` cells) periodically emit `SporeGas` above them; a `SporeGas` cell orthogonally adjacent to `Soil` has a `P_RESEED_CHANCE` per growth tick to convert that soil to a fresh mycelium seed (enqueued on the frontier), consuming the spore. `World::puff_and_reseed()`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/sandgun-core/tests/growth.rs`:
```rust
#[test]
fn spore_adjacent_to_soil_reseeds_a_colony() {
    let mut w = World::new(64, 64);
    w.set_param(sandgun_core::params::P_RESEED_CHANCE as u32, 1.0); // force reseed
    for x in 0..64 {
        w.paint(x, 40, 0, Material::Soil as u8);
    }
    w.paint(20, 39, 0, Material::SporeGas as u8); // a spore resting on the soil surface
    // seed_frontier finds no mycelium, but puff_and_reseed still runs each growth tick
    w.seed_frontier();
    let before = mycelium_count(&w);
    for _ in 0..60 {
        w.step();
    }
    assert!(mycelium_count(&w) > before, "a spore on soil should seed new mycelium");
    assert!(w.frontier_len() >= 1, "the reseeded cell should join the frontier");
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p sandgun-core --test growth`
Expected: FAIL — no reseed logic; also note `grow()` currently early-returns when the frontier is empty, so `puff_and_reseed` must run regardless of an empty frontier when spores exist. Adjust the guard (Step 3).

- [ ] **Step 3: Implement puff + reseed**

Adjust `grow()`'s early-return so reseeding still happens when spores are present but the frontier is empty. Replace the guard at the top of `grow()`:
```rust
        // Nothing living AND nothing to reseed -> truly idle.
        if self.frontier.is_empty() && self.mushrooms.is_empty() {
            self.puff_and_reseed(); // cheap scan-free pass; no-ops if there are no spores near soil
            if self.frontier.is_empty() {
                return;
            }
        }
```
Wait — a bare `puff_and_reseed` that scans the whole world every growth tick reintroduces world-size cost and breaks chunk-sleep. Instead, reseeding must be driven off spores as they settle, not a full scan. Implement it as: whenever a `SporeGas` cell is *processed by the falling-sand sweep* and finds itself resting on soil, roll reseed. Add to the gas-update path in `world.rs` (find where `SporeGas`/gas cells are updated in `update_cell`/the gas handler) a hook:
```rust
        // M1c: a spore resting against soil may seed a new colony.
        if mat == Material::SporeGas {
            self.try_reseed(x, y);
        }
```
And implement in `growth.rs`:
```rust
    /// If this spore touches soil, maybe convert that soil to a mycelium seed (and consume the spore).
    pub(crate) fn try_reseed(&mut self, x: usize, y: usize) {
        if !self.chance(self.params.values[P_RESEED_CHANCE]) {
            return;
        }
        for (nx, ny) in self.ortho(x, y) {
            if self.material_at(nx, ny) == Material::Soil {
                let (ux, uy) = (nx as usize, ny as usize);
                self.set_mycelium(ux, uy);
                self.frontier.push(FrontierCell { x: ux, y: uy, reach: 0 });
                // consume the spore
                let si = self.idx(x, y);
                self.cells[si].material = Material::Empty as u8;
                self.wake(x, y);
                return;
            }
        }
    }
```
Revert the `grow()` guard to the simple form (reseed is now sweep-driven, not scan-driven):
```rust
        if self.frontier.is_empty() && self.mushrooms.is_empty() {
            return;
        }
```
For **cap puffs**: puffing must be **finite** so the world can eventually sleep (a cap that puffs forever would keep spawning spores and never let `cells_processed` hit 0 — a kill-criterion failure). When a mushroom retires (fully grown) in `grow_mushrooms`, record its cap center in a `caps` field carrying (x, y, puff_countdown, **remaining_puffs**): `pub(crate) caps: Vec<(usize, usize, u32, u8)>` on `World` (init `Vec::new()`; push `(cx, cy, P_PUFF_INTERVAL as u32, 3)` on completion — 3 puffs then the cap goes dormant). `puff_caps` decrements each countdown; at 0 it emits `P_PUFF_SPORES` `SporeGas` cells above the cap (only into `Empty`), resets the countdown, and decrements `remaining_puffs`; a cap with `remaining_puffs == 0` is `swap_remove`d. So `caps` drains to empty and the world can sleep. Split the tuple read from the mutation to satisfy the borrow checker:
```rust
    fn puff_caps(&mut self) {
        let interval = (self.params.values[P_PUFF_INTERVAL] as u32).max(1);
        let spores = self.params.values[P_PUFF_SPORES] as i32;
        let mut k = 0;
        while k < self.caps.len() {
            let (cx, cy, cd, rem) = self.caps[k];
            if cd == 0 {
                let (cxi, cyi) = (cx as i32, cy as i32);
                for s in 0..spores {
                    let px = cxi + (s - spores / 2);
                    let py = cyi - 1;
                    if self.in_bounds(px, py) && self.material_at(px, py) == Material::Empty {
                        let idx = self.idx(px as usize, py as usize);
                        self.cells[idx].material = Material::SporeGas as u8;
                        self.cells[idx].aux = Material::SporeGas.initial_aux();
                        self.wake(px as usize, py as usize);
                    }
                }
                if rem <= 1 {
                    self.caps.swap_remove(k); // dormant — cap is done puffing, lets the world sleep
                    continue;
                }
                self.caps[k].2 = interval;
                self.caps[k].3 = rem - 1;
            } else {
                self.caps[k].2 = cd - 1;
            }
            k += 1;
        }
    }
```
Call `self.puff_caps();` inside `grow()` after `grow_mushrooms()`. **Also fold `caps` into `grow()`'s early-return guard** so caps keep puffing while active but stop costing work once drained — replace the guard at the top of `grow()`:
```rust
        if self.frontier.is_empty() && self.mushrooms.is_empty() && self.caps.is_empty() {
            return;
        }
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p sandgun-core --test growth` then `cargo test -p sandgun-core`
Expected: all pass. If chunk-sleep tests regress, `try_reseed`/`puff_caps` is running when it shouldn't — confirm `try_reseed` only fires from the spore sweep path and `puff_caps` only touches existing caps.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: caps puff spores; spores on soil reseed new colonies (M1c task 5)"
```

---

### Task 6: Shooting a cap dumps its spore cloud

**Files:**
- Modify: `crates/sandgun-core/src/world.rs` (impact-on-MushroomFlesh releases a spore puff)
- Modify: `crates/sandgun-core/tests/growth.rs`

**Interfaces:**
- Consumes: the projectile impact path (`on_impact`/`carve_crater` in `world.rs` from M1b), `P_PUFF_SPORES`.
- Produces: any projectile impact that carves `MushroomFlesh` also releases a burst of `SporeGas` around the impact (which then drifts/reseeds via Task 5, or detonates if fire is present via the existing M1a spore-detonation). No new "accumulated cloud" state — the burst is proportional to `P_PUFF_SPORES`.

- [ ] **Step 1: Write the failing test**

Append to `crates/sandgun-core/tests/growth.rs`:
```rust
#[test]
fn shooting_mushroom_flesh_releases_spores() {
    let mut w = World::new(64, 64);
    // a slab of mushroom flesh
    for x in 28..36 {
        for y in 28..36 {
            w.paint(x, y, 0, Material::MushroomFlesh as u8);
        }
    }
    let spores_before = spore_count(&w);
    // fire a kinetic round into the slab
    w.fire(5.0, 32.0, 12.0, 0.0, 0); // Kinetic = 0
    for _ in 0..30 {
        w.step();
    }
    assert!(spore_count(&w) > spores_before, "popping mushroom flesh should release spore gas");
}

fn spore_count(w: &World) -> usize {
    let mut n = 0;
    for y in 0..w.height {
        for x in 0..w.width {
            if w.get(x, y) == Material::SporeGas {
                n += 1;
            }
        }
    }
    n
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p sandgun-core --test growth`
Expected: FAIL — impact carves flesh but releases no spores.

- [ ] **Step 3: Release spores when an impact touches flesh**

In `world.rs`, in the payload impact path (where `on_impact`/`carve_crater` runs — locate the kinetic carve that removes cells), after carving, add a flesh-detection puff. The simplest correct hook: in `carve_crater`, when the carved cell was `MushroomFlesh`, spawn a `SporeGas` cell in its place a fraction of the time. Find the carve loop that clears cells within the radius and, where it identifies the material being removed, add:
```rust
                if removed == Material::MushroomFlesh
                    && self.chance((self.params.values[crate::params::P_PUFF_SPORES] / 8.0).min(1.0))
                {
                    let idx = self.idx(cx, cy);
                    self.cells[idx].material = Material::SporeGas as u8;
                    self.cells[idx].aux = Material::SporeGas.initial_aux();
                    self.wake(cx, cy);
                    continue; // leave spore gas instead of empty this cell
                }
```
(`removed` = the `Material` read from the cell before carving; adapt the variable name to the existing carve code. The `chance` ties spore density to `P_PUFF_SPORES` so it stays hot-tunable. Near fire, these `SporeGas` cells detonate via the existing M1a path — no extra code.)

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p sandgun-core --test growth` then `cargo test -p sandgun-core`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: shooting mushroom flesh releases a spore cloud (M1c task 6)"
```

---

### Task 7: WASM counts + debug HUD + combined chunk-sleep guard + acceptance

**Files:**
- Modify: `crates/sandgun-core/src/world.rs` (seed frontier after generate; count accessors)
- Modify: `crates/sandgun-wasm/src/lib.rs` (passthroughs)
- Modify: `web/src/overlay.js` (HUD counts)
- Modify: `web/src/main.js` (nothing required if generate() seeds internally — verify)
- Modify: `crates/sandgun-core/tests/growth.rs` (combined chunk-sleep regression)

**Interfaces:**
- Consumes: everything above.
- Produces: worldgen auto-seeds the frontier so the living world starts moving on load; `WasmWorld::frontier_count()`/`mushroom_count()`; debug HUD line shows `frontier N · mush M`; the combined regression guard for M1c.

- [ ] **Step 1: Seed the frontier at generation time**

In `world.rs`, at the END of the `generate`-driving method (find where `worldgen::generate(self, seed)` is called — likely a `World::generate(&mut self, seed)` wrapper; if generation is only in the `worldgen` module, add the seed call in the `WasmWorld::generate` wrapper instead), append:
```rust
        self.seed_frontier();
```
so a freshly generated world's mycelium is already on the frontier.

- [ ] **Step 2: Write the failing combined regression test**

Append to `crates/sandgun-core/tests/growth.rs`:
```rust
#[test]
fn full_lifecycle_world_still_sleeps_after_settling() {
    // avatar + projectile + particles + active growth all at once, then everything must settle to sleep.
    let mut w = soil_world();
    w.paint(64, 90, 0, Material::Mycelium as u8);
    w.seed_frontier();
    w.spawn_avatar(60.0, 70.0);
    w.fire(5.0, 85.0, 12.0, 0.0, 0);
    w.spawn_particle(40.0, 60.0, 0.5, 0.0, Material::Sand as u8);
    for _ in 0..4000 {
        w.step();
    }
    assert_eq!(w.frontier_len(), 0, "growth must terminate");
    assert_eq!(w.mushroom_len(), 0, "mushrooms must finish");
    w.step();
    assert_eq!(w.cells_processed, 0, "full living world must return to sleep once settled");
}
```

- [ ] **Step 3: Run to verify fail, then add accessors + wasm passthroughs**

Run: `cargo test -p sandgun-core --test growth` — the new test likely fails first because accessors/seed exist but confirm it *settles*; if it never sleeps, growth isn't terminating (diagnose: caps puff forever ⇒ spores ⇒ reseed ⇒ new frontier is EXPECTED to eventually exhaust the finite soil; if soil is infinite-ish the world legitimately keeps growing — in this enclosed `soil_world` the 128×20 soil band is finite, so it must terminate. If it doesn't, cap puffing is generating perpetual spores with no soil left — guard `puff_caps` to skip caps with no reachable soil, or accept that caps stop puffing after `P_PUFF_INTERVAL * k`; simplest: cap total puffs per mushroom to a small count stored in the `caps` tuple).

In `crates/sandgun-wasm/src/lib.rs`, add passthroughs:
```rust
    pub fn frontier_count(&self) -> usize {
        self.inner.frontier_len()
    }
    pub fn mushroom_count(&self) -> usize {
        self.inner.mushroom_len()
    }
```

- [ ] **Step 4: HUD counts**

In `web/src/overlay.js`, extend the debug line (only when `input.debug`) to include growth state:
```js
  const growth = input.debug ? ` · frontier ${world.frontier_count()} · mush ${world.mushroom_count()}` : '';
  octx.fillText(`${fps.toFixed(0)} fps · ${rate} · ${input.status}${gun ? ` · ${gun.status}` : ''}${input.debug ? ` · ${world.cells_processed()} cells${growth}` : ''}`, 6, 12);
```

- [ ] **Step 5: Build wasm + full test run**

Run:
```bash
cargo test -p sandgun-core
./scripts/build-wasm.sh
```
Expected: all tests pass; wasm builds.

- [ ] **Step 6: Browser acceptance (the M1c kill criterion)**

Run `cd web && npm run dev`, open the page, press `` ` `` for the debug HUD, then verify:
1. On load the world is alive — `frontier` count is non-zero and mycelium visibly creeps across soil at a watchable crawl (seconds).
2. **Shoot a hole** in a mycelium patch (kinetic, `Z`); over ~10–30s the frontier **creeps back into the crater** (mycelium bridges the empty).
3. A mature patch **fruits a mushroom** that grows stem-then-cap over seconds; the `mush` count rises then falls as it completes.
4. **Shoot a cap** (`Z`) → a spore cloud puffs out; if you light it (`X` incendiary nearby) it detonates.
5. Leave it running until growth settles (frontier → 0): the FPS stays ~60 and, with the HUD showing `0 cells` when nothing moves, **the world sleeps** (no busy-spin from perpetual growth).
6. Capture FPS during active growth + fire; note it in the report. If growth never lets the world sleep, that's a kill-criterion failure — diagnose termination before declaring done.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: seed frontier on worldgen, growth HUD, lifecycle sleep guard — M1c acceptance (M1c task 7)"
```

---

## Self-review notes

- Spec coverage vs the M1c design doc: colonize soil ✓ (T1), bridge craters range-limited ✓ (T2), water accel ✓ (T2), maturity in aux + fruiting cap ✓ (T3), parametric stem+cap cell-by-cell ✓ (T4), puff + reseed ✓ (T5), shoot-cap dumps cloud + fire detonation reuse ✓ (T6), animate existing worldgen ✓ (T7), budgeted frontier every N frames ✓ (T1), chunk-sleep sacred + terminal-state guard ✓ (T1 + T7 combined test).
- Reach-from-soil on the `FrontierCell` entry, maturity in `aux` — the two-counters-one-byte reconciliation is honored (T1/T2 reach; T3 aux).
- Determinism: all rolls via `next_rand`/`chance`/`rand_range`; `cap_disk` is a deterministic ordered set. No wall-clock.
- Params: all 16 growth params added Rust-side AND mirrored in `params.json`/`params.js` in the SAME task (T1) to avoid the M1b desync finding.
- **Known risk flagged for the implementer:** growth termination. The combined sleep test (T7) is the guard; if perpetual cap-puffing → reseed keeps the world awake on finite soil, cap total puffs per mushroom (store a remaining-puffs count in the `caps` tuple). Do not weaken the sleep assertion to pass — fix termination.
- Deferred (not M1c): big 1024×2048 world + follow-cam (M1d), enemies, roguelite, native build, audio.
```
