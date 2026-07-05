# SANDGUN M1e v1 — "Living Mycelium" Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace M1c's fill-a-blob growth with a living organism: thin hyphal strands, grown by bounded **tips**, gradient-seek per-cell substrate richness; a **colony** eats substrate to fill a nutrient pool, fruits when fed (spending the pool), starves back when not; anything cut off from the ground falls and dies. Design spec: `docs/superpowers/specs/2026-07-05-m1e-living-mycelium-design.md`.

**Architecture:** New module `crates/sandgun-core/src/mycelium.rs` holds `Colony`/`Tip` structs and the `impl World` growth methods. Grid cells stay the source of truth: **soil `aux` = substrate richness (0–255)**, **mycelium `aux` = colony id (1–255)** (different materials, same byte, never both at once). Side state on `World`: `colonies: Vec<Colony>`, `tips: Vec<Tip>`. Growth cost is O(live tips), not world size. Support/anchor is computed by a **budgeted local flood on carve/burn only** (mycelium never moves on its own, so connectivity only changes when cells are grown or destroyed). Reuses the existing parametric mushroom shape (`reveal_mushroom`/`mushroom_footprint`/`cap_dome`/`stem_dx`) and pixels-as-particles (for falling).

**Strategy:** Build the new model incrementally with the OLD model kept but **dormant** (Task 1 stops worldgen/paint from seeding the old frontier, so `grow()` no-ops). Each task is tested via a `spawn_colony` helper, not worldgen. Task 6 does the switchover + rips out the old code. This keeps the two models from fighting and concentrates the deletion after the new model is proven.

## Global Constraints

- Cell stays 4 bytes; flags bit 7 reserved (M2 rigid bodies); FLAG_BURNING = bit 1. NO per-cell temperature.
- **`aux` semantics (M1e):** on a `Soil` cell, `aux` = substrate richness (0–255). On a `Mycelium` cell, `aux` = colony id (1–255; 0 = unassigned). On a burning cell, `aux` = fuel (unchanged — fire owns aux once FLAG_BURNING is set). These never coincide on one cell. On `MushroomFlesh`, `aux` unused (0).
- **Chunk sleeping is SACRED.** Growth processes only live tips; when no colony has live tips and nothing is falling, the growth pass early-returns and the world sleeps (`cells_processed == 0`). Substrate richness changes are event-driven (consumption; v2 water/ash) — never a per-frame world scan. The support flood runs only on carve/burn, budgeted and local. Every cell mutation calls `wake()`.
- **Determinism:** all growth/seek/branch/wander/fruit randomness via `World::next_rand`/`chance`/`rand_range`. No wall-clock.
- Sim logic only in `sandgun-core`; `sandgun-wasm` glue-only. Coordinates: grid is `usize`/`isize`; `+y` down; existing `idx`, `in_bounds`, `material_at`, `get`, `wake`, `next_rand`, `chance`, `rand_range`, `cell_aux`, `cell_flags` on `World`.
- Material ids unchanged: `Empty=0 Rock=1 Sand=2 Water=3 Oil=4 Soil=5 Mycelium=6 MushroomFlesh=7 SporeGas=8 Smoke=9 Ash=10 Acid=11 Fire=12`.
- Params: `params.rs` indices mirrored in `web/src/params.js` + `web/public/params.json` — keep all three in sync in the SAME task that adds/removes a param.
- All commits end with: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` (second `-m`). Run cargo from repo root. Rebuild wasm with `./scripts/build-wasm.sh`.
- Work on a branch: `git checkout -b m1e-living-mycelium` before Task 1.

## What is KEPT vs REMOVED from M1c growth

- **KEEP** (used by new fruiting/rendering): `GrowingMushroom`, `reveal_mushroom`, `mushroom_footprint`, `cap_dome`, `stem_dx`, `has_fruiting_room`, `rand_range`. The `mushrooms: Vec<GrowingMushroom>` field and `grow_mushrooms` reveal loop stay.
- **REMOVE** (Task 6): `FrontierCell`, `frontier`/`caps`/`frontier_drops` fields, `seed_frontier`, `seed_frontier_around`, `has_colonizable_neighbor`, `has_colonizable_neighbor_or_bridge`, `colonize_from`, old `set_mycelium`, `try_reseed`, old `grow` (frontier loop + cap puffs), old `try_fruit` (aux-maturity trigger), `water_adjacent`, `shuffled_ortho`, `push_frontier`, and the old growth params (`P_MAX_FRONTIER`, `P_MAX_REACH`, `P_WATER_ACCEL`, `P_MATURITY`, `P_FRUIT_CHANCE`, `P_PUFF_INTERVAL`, `P_PUFF_SPORES`, `P_RESEED_CHANCE`). Delete their tests.

---

### Task 1: Substrate richness + colony/tip scaffolding; old model dormant

**Files:**
- Create: `crates/sandgun-core/src/mycelium.rs`
- Create: `crates/sandgun-core/tests/mycelium.rs`
- Modify: `crates/sandgun-core/src/lib.rs` (`pub mod mycelium;`)
- Modify: `crates/sandgun-core/src/world.rs` (fields, richness accessors, dormant old model)
- Modify: `crates/sandgun-core/src/worldgen.rs` (bake soil richness; stop seeding old frontier)
- Modify: `crates/sandgun-core/src/params.rs` (add new M1e params)
- Modify: `web/public/params.json`, `web/src/params.js` (mirror new params)

**Interfaces produced:**
- `Colony { id: u8, nutrient_pool: u32, tip_count: u16, alive: bool }`; `Tip { x: usize, y: usize, colony: u8, last_dx: i8, last_dy: i8, alive: bool }`.
- `World.colonies: Vec<Colony>`, `World.tips: Vec<Tip>`.
- `World::soil_richness(x,y) -> u8` / `World::set_soil_richness(x,y,v)`; `World::spawn_colony(x,y) -> u8` (creates a colony on a mycelium cell at (x,y) with one tip; returns colony id); `World::colony_count()`, `World::tip_count()`, `World::colony_pool(id) -> u32`.
- `World::grow_mycelium()` — the new growth entry point (Task 1: no-op when no live tips).
- New params: `P_MY_GROWTH_INTERVAL`(frames/tick), `P_MY_TIP_CAP`(max live tips/colony), `P_MY_EAT`(richness→pool multiplier), `P_MY_FRUIT_COST`(pool spent per fruiting), `P_MY_FRUIT_THRESHOLD`(pool to fruit), `P_MY_DIEBACK`(dieback rate), `P_MY_BRANCH_CHANCE`(periodic branch chance), `P_MY_WORLDGEN_COLONIES`(colony origins at gen), `P_SOIL_RICHNESS_MIN`/`MAX`(worldgen baseline). Keep `P_MUSH_*`, `P_MUSH_REVEAL`, `P_ASH_CHANCE`, `P_GUNFIRE_SPORE_CHANCE`.

- [ ] **Step 1: Params — remove old frontier params, add M1e params (rs + json + js in sync)**

In `params.rs`: delete `P_MAX_FRONTIER, P_MAX_REACH, P_WATER_ACCEL, P_MATURITY, P_FRUIT_CHANCE, P_PUFF_INTERVAL, P_PUFF_SPORES, P_RESEED_CHANCE` and their `Default` lines. Renumber remaining indices contiguously and add the new M1e params at the end. Final set (renumber exactly, contiguous 0..P_COUNT): keep fire/smoke/flam/fuel/acid/kinetic/incendiary/acid_blob/spore_blob/mush_height/mush_cap/mush_reveal/gunfire_spore/ash, then add:
```rust
pub const P_MY_GROWTH_INTERVAL: usize = /*next*/;
pub const P_MY_TIP_CAP: usize = /*..*/;
pub const P_MY_EAT: usize = /*..*/;
pub const P_MY_FRUIT_THRESHOLD: usize = /*..*/;
pub const P_MY_FRUIT_COST: usize = /*..*/;
pub const P_MY_DIEBACK: usize = /*..*/;
pub const P_MY_BRANCH_CHANCE: usize = /*..*/;
pub const P_MY_WORLDGEN_COLONIES: usize = /*..*/;
pub const P_SOIL_RICHNESS_MIN: usize = /*..*/;
pub const P_SOIL_RICHNESS_MAX: usize = /*..*/;
pub const P_COUNT: usize = /*..*/;
```
Defaults: `P_MY_GROWTH_INTERVAL=3`, `P_MY_TIP_CAP=12`, `P_MY_EAT=1.0`, `P_MY_FRUIT_THRESHOLD=400`, `P_MY_FRUIT_COST=350`, `P_MY_DIEBACK=1`, `P_MY_BRANCH_CHANCE=0.04`, `P_MY_WORLDGEN_COLONIES=6`, `P_SOIL_RICHNESS_MIN=40`, `P_SOIL_RICHNESS_MAX=120`. Mirror EVERY param (names + indices) into `web/public/params.json` and `web/src/params.js` so all three agree and `P_COUNT` matches the JS index count.
(Any code referencing a removed param constant will fail to compile — that's expected; Task 6 removes those call sites. For Task 1, temporarily keeping the old `grow()` compiling may require leaving old param constants in place. SIMPLER: in Task 1, do NOT delete old params yet — only ADD the new ones (bump P_COUNT); Task 6 deletes the old ones together with the old code. Do it that way to keep the build green.)

- [ ] **Step 2: Write failing tests** — `crates/sandgun-core/tests/mycelium.rs`:
```rust
use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn soil_richness_roundtrips() {
    let mut w = World::new(64, 64);
    w.paint(10, 10, 0, Material::Soil as u8);
    w.set_soil_richness(10, 10, 100);
    assert_eq!(w.soil_richness(10, 10), 100);
}

#[test]
fn spawn_colony_makes_a_colony_with_one_tip() {
    let mut w = World::new(64, 64);
    let id = w.spawn_colony(32, 32);
    assert_eq!(w.get(32, 32), Material::Mycelium);
    assert_eq!(w.colony_count(), 1);
    assert_eq!(w.tip_count(), 1);
    assert!(id >= 1);
}

#[test]
fn no_tips_means_grow_is_noop_and_world_sleeps() {
    let mut w = World::new(128, 128);
    // some settled sand, no colonies
    for x in 0..128 { w.paint(x, 100, 0, Material::Rock as u8); }
    for _ in 0..50 { w.step(); }
    w.step();
    assert_eq!(w.cells_processed, 0);
    assert_eq!(w.tip_count(), 0);
}
```

- [ ] **Step 3: Run to verify fail** — `cargo test -p sandgun-core --test mycelium` → FAIL (module/methods missing).

- [ ] **Step 4: Implement scaffolding**

`lib.rs`: add `pub mod mycelium;`.

`mycelium.rs`:
```rust
use crate::cell::Material;
use crate::world::World;

#[derive(Clone, Copy)]
pub struct Colony {
    pub id: u8,
    pub nutrient_pool: u32,
    pub tip_count: u16,
    pub alive: bool,
}

#[derive(Clone, Copy)]
pub struct Tip {
    pub x: usize,
    pub y: usize,
    pub colony: u8,
    pub last_dx: i8,
    pub last_dy: i8,
    pub alive: bool,
}

impl World {
    pub fn soil_richness(&self, x: usize, y: usize) -> u8 {
        if self.get(x, y) == Material::Soil { self.cell_aux(x, y) } else { 0 }
    }
    pub fn set_soil_richness(&mut self, x: usize, y: usize, v: u8) {
        let i = self.idx(x, y);
        if Material::from_u8(self.cells[i].material) == Material::Soil {
            self.cells[i].aux = v;
        }
    }
    /// Create a colony rooted at (x,y): lay a mycelium cell (aux=colony id) and one tip.
    pub fn spawn_colony(&mut self, x: usize, y: usize) -> u8 {
        let id = (self.colonies.len() as u8).wrapping_add(1); // 1-based; v1 assumes < 255 colonies
        self.colonies.push(Colony { id, nutrient_pool: 0, tip_count: 1, alive: true });
        let i = self.idx(x, y);
        self.cells[i].material = Material::Mycelium as u8;
        self.cells[i].aux = id;
        self.cells[i].flags &= !crate::cell::FLAG_BURNING;
        self.wake(x, y);
        self.tips.push(Tip { x, y, colony: id, last_dx: 0, last_dy: -1, alive: true });
        id
    }
    pub fn colony_count(&self) -> usize { self.colonies.iter().filter(|c| c.alive).count() }
    pub fn tip_count(&self) -> usize { self.tips.iter().filter(|t| t.alive).count() }
    pub fn colony_pool(&self, id: u8) -> u32 {
        self.colonies.iter().find(|c| c.id == id).map(|c| c.nutrient_pool).unwrap_or(0)
    }
    /// New growth entry point. Task 1: no-op when no live tips (chunk-sleep safe).
    pub fn grow_mycelium(&mut self) {
        if self.tips.iter().all(|t| !t.alive) { return; }
        // tip processing added in Task 2
    }
}
```

`world.rs`: add fields `pub(crate) colonies: Vec<Colony>` and `pub(crate) tips: Vec<Tip>` (import from `crate::mycelium`); init `Vec::new()` in `new()` and clear them in `clear()`. In `step()`, ADD a call to `self.grow_mycelium();` right after the existing old-growth cadence hook (both run; old is dormant per Step 5). 

- [ ] **Step 5: Make the old model dormant** — In `worldgen.rs`: replace the `world.seed_frontier();` call (line ~247) with baking soil richness instead: after terrain is generated, for every `Soil` cell set `aux` to `rng.range(P_SOIL_RICHNESS_MIN.., P_SOIL_RICHNESS_MAX..)` (read the param values). Do NOT call `seed_frontier`. In `world.rs::paint`, remove the `seed_frontier_around` call for painted Mycelium (the new model doesn't use the frontier; painted mycelium behavior is redefined in Task 6/handled by colonies). The old `grow()` still runs in step() but with an always-empty frontier it no-ops. (Old growth tests that call `seed_frontier` directly still pass for now.)

- [ ] **Step 6: Run tests** — `cargo test -p sandgun-core --test mycelium` (3 pass) and full `cargo test -p sandgun-core` (old growth tests still pass since they seed the frontier manually; build green). `./scripts/build-wasm.sh` succeeds.

- [ ] **Step 7: Commit** — `git add -A && git commit -m "feat: M1e scaffolding — substrate richness, colony/tip structs, old growth dormant (task 1)"`

---

### Task 2: Tip seek & extend + eat substrate

**Files:** Modify `mycelium.rs` (`grow_mycelium` tip loop, `extend_tip`, `pick_step`), `world.rs` (wire cadence — already calls grow_mycelium), `tests/mycelium.rs`.

**Interfaces produced:** `grow_mycelium` now processes each live tip once per growth tick (on the `P_MY_GROWTH_INTERVAL` cadence): the tip samples substrate richness in its 8-neighborhood, steps toward the richest reachable cell (Empty or Soil; not solid/rock/other), lays a `Mycelium` cell (aux = colony id), and if it stepped into `Soil`, consumes that soil's richness into the colony pool (`pool += richness * P_MY_EAT`, soil richness spent). Random wander tie-breaks.

- [ ] **Step 1: Failing tests**:
```rust
#[test]
fn tip_grows_toward_richer_substrate() {
    let mut w = World::new(64, 64);
    // rich soil to the right of the colony, poor/empty to the left
    for x in 34..50 { for y in 30..34 { w.paint(x as i32, y as i32, 0, Material::Soil as u8); w.set_soil_richness(x, y, 200); } }
    w.spawn_colony(32, 32);
    for _ in 0..200 { w.step(); }
    // mycelium should have advanced rightward into the rich soil
    let reached = (34..50).any(|x| w.get(x, 32) == Material::Mycelium || w.get(x, 33) == Material::Mycelium);
    assert!(reached, "tip should creep toward the rich soil");
}

#[test]
fn eating_soil_fills_the_colony_pool() {
    let mut w = World::new(64, 64);
    for x in 30..50 { for y in 30..40 { w.paint(x as i32, y as i32, 0, Material::Soil as u8); w.set_soil_richness(x, y, 150); } }
    let id = w.spawn_colony(32, 35);
    let before = w.colony_pool(id);
    for _ in 0..100 { w.step(); }
    assert!(w.colony_pool(id) > before, "eating rich soil should fill the pool");
}
```

- [ ] **Step 2: Run to verify fail.**

- [ ] **Step 3: Implement tip growth** in `mycelium.rs` (cadence gate lives in world.rs step() already; here process all live tips once per call):
```rust
    pub fn grow_mycelium(&mut self) {
        if self.tips.iter().all(|t| !t.alive) { return; }
        let eat = self.params.values[crate::params::P_MY_EAT];
        for ti in 0..self.tips.len() {
            if !self.tips[ti].alive { continue; }
            self.extend_tip(ti, eat);
        }
        self.tips.retain(|t| t.alive); // drop dead tips so the loop stays cheap
    }

    fn extend_tip(&mut self, ti: usize, eat: f32) {
        let t = self.tips[ti];
        let Some((nx, ny)) = self.pick_step(t) else { self.tips[ti].alive = false; return; };
        let dst = self.material_at(nx, ny);
        let (ux, uy) = (nx as usize, ny as usize);
        // eat if stepping into soil
        if dst == Material::Soil {
            let r = self.cell_aux(ux, uy) as f32;
            if let Some(c) = self.colonies.iter_mut().find(|c| c.id == t.colony) {
                c.nutrient_pool = c.nutrient_pool.saturating_add((r * eat) as u32);
            }
        }
        let i = self.idx(ux, uy);
        self.cells[i].material = Material::Mycelium as u8;
        self.cells[i].aux = t.colony; // colony id; overwrites soil richness (now spent)
        self.cells[i].flags &= !crate::cell::FLAG_BURNING;
        self.wake(ux, uy);
        self.tips[ti].x = ux;
        self.tips[ti].y = uy;
        self.tips[ti].last_dx = (nx - t.x as isize) as i8;
        self.tips[ti].last_dy = (ny - t.y as isize) as i8;
    }

    /// Choose the next cell for a tip: the passable (Empty/Soil) neighbor with the highest
    /// substrate richness, momentum-biased and RNG-tie-broken. None if boxed in.
    fn pick_step(&mut self, t: Tip) -> Option<(isize, isize)> {
        let (tx, ty) = (t.x as isize, t.y as isize);
        let mut best: Option<(isize, isize)> = None;
        let mut best_score = i32::MIN;
        // evaluate the 8 neighbors in a shuffled order for unbiased ties
        let mut order = [0u8, 1, 2, 3, 4, 5, 6, 7];
        for k in (1..8).rev() { let j = (self.next_rand() as usize) % (k + 1); order.swap(k, j); }
        const D: [(isize, isize); 8] = [(-1,-1),(0,-1),(1,-1),(-1,0),(1,0),(-1,1),(0,1),(1,1)];
        for &oi in order.iter() {
            let (dx, dy) = D[oi as usize];
            let (nx, ny) = (tx + dx, ty + dy);
            if !self.in_bounds(nx, ny) { continue; }
            let m = self.material_at(nx, ny);
            if m != Material::Empty && m != Material::Soil { continue; } // only grow into empty/soil
            let richness = if m == Material::Soil { self.cell_aux(nx as usize, ny as usize) as i32 } else { 0 };
            // momentum bias: prefer continuing roughly the same direction (thin strands, not blobs)
            let momentum = if dx == t.last_dx as isize && dy == t.last_dy as isize { 6 } else { 0 };
            let score = richness + momentum + (self.next_rand() % 3) as i32;
            if score > best_score { best_score = score; best = Some((nx, ny)); }
        }
        best
    }
```
NOTE the momentum bias + "only grow into empty/soil" makes strands thin and directional (not a fill). No max-reach: strands may cross empty (exploring); support/anchor is enforced only on carve (Task 5) and by starvation (Task 3).

- [ ] **Step 4: Run tests** — new pass; `cargo test -p sandgun-core`.
- [ ] **Step 5: Commit** — `feat: hyphal tips gradient-seek and eat substrate (M1e task 2)`

---

### Task 3: Branching + tip cap + starvation dieback

**Files:** Modify `mycelium.rs`, `tests/mycelium.rs`.

**Interfaces:** In `grow_mycelium`: after extending, a tip that stepped into rich soil (richness above a small floor) spawns 1–2 new tips (biased toward nearby richness) AND every tip has `P_MY_BRANCH_CHANCE` to branch — both gated by the colony's live tip count `< P_MY_TIP_CAP`. Starvation: if a colony's `nutrient_pool == 0`, its tips don't extend; instead the colony recedes — mark the frontier-most mycelium cells back to `Empty` from the tips inward at rate `P_MY_DIEBACK` per tick (dieback), and kill tips that can't extend. A colony with no live tips and empty pool is `alive = false` once fully receded.

- [ ] **Step 1: Failing tests**:
```rust
#[test]
fn well_fed_colony_branches_up_to_the_cap() {
    let mut w = World::new(96, 96);
    for x in 10..90 { for y in 40..60 { w.paint(x as i32, y as i32, 0, Material::Soil as u8); w.set_soil_richness(x, y, 220); } }
    w.spawn_colony(48, 50);
    for _ in 0..300 { w.step(); }
    let cap = 12; // P_MY_TIP_CAP default
    assert!(w.tip_count() > 1, "a fed colony should branch");
    assert!(w.tip_count() <= cap, "tips must not exceed the cap");
}

#[test]
fn starving_colony_recedes_and_world_sleeps() {
    let mut w = World::new(64, 64);
    // colony in EMPTY space (no soil to eat) -> pool stays 0 -> starves
    w.spawn_colony(32, 32);
    for _ in 0..400 { w.step(); }
    w.step();
    assert_eq!(w.tip_count(), 0, "starved tips die");
    assert_eq!(w.cells_processed, 0, "receded, settled world sleeps");
}
```

- [ ] **Step 2..5:** Implement branching (respect per-colony cap via `colony.tip_count`; keep `tip_count` in sync as tips spawn/die), and starvation dieback (when `pool == 0`, receding: convert the tip's own cell back to Empty stepping inward, kill the tip; a colony fully receded → `alive=false`). Ensure the receded world sleeps (dieback finishes, no live tips). Run tests, commit: `feat: strand branching (capped) and starvation dieback (M1e task 3)`.

---

### Task 4: Fruiting from the colony economy + simple mushroom decay

**Files:** Modify `mycelium.rs` (fruiting trigger), `world.rs` (mushroom decay in the reveal/aging path), `tests/mycelium.rs`.

**Interfaces:** In `grow_mycelium`, after tip processing: for each colony with `nutrient_pool >= P_MY_FRUIT_THRESHOLD`, pick a well-fed tip (any live tip of that colony that `has_fruiting_room`), fruit a parametric mushroom there via the KEPT `try_fruit`-equivalent (reuse `mushroom_footprint` fit-check + push `GrowingMushroom`), and subtract `P_MY_FRUIT_COST` from the pool. Reuse `grow_mushrooms`/`reveal_mushroom` unchanged for the reveal. **Simple v1 decay:** a completed mushroom gets a lifespan; after it expires, its `MushroomFlesh` cells crumble — each becomes `Ash` with chance `P_ASH_CHANCE` else `Empty` (reuse the burn-ash logic), waking cells. Track completed-mushroom lifespans in a small bounded list (like the old `caps`, but for decay) so it stays chunk-sleep-safe and drains.

- [ ] Tests: `fed_colony_fruits_and_spends_pool` (pool drops by ~cost after fruiting; mushroom_len rises), `hungry_colony_does_not_fruit` (pool below threshold → no mushroom), `mushroom_decays_after_lifespan` (a grown mushroom's flesh count drops to ~0 after its lifespan, world still sleeps). Verify fail-pre/pass-post. Commit: `feat: colony fruits when fed (spends pool); mushrooms decay (M1e task 4)`.

---

### Task 5: Support/anchor — carve-flood; unsupported chunks fall & die

**Files:** Modify `world.rs` (hook carve/burn removal of mycelium/mushroom → support check), `mycelium.rs` (`drop_unsupported_around`), `tests/mycelium.rs`.

**Interfaces:** `World::drop_unsupported_around(cx, cy, radius)` — after mycelium/MushroomFlesh cells are removed by a carve or burn, run a **budgeted local BFS** over the connected `Mycelium`/`MushroomFlesh` cells reachable from the affected region; for each connected group, if NO cell in it is orthogonally adjacent to an anchor (`Rock` or `Soil`), the whole group is unsupported → convert each of its cells to a falling **particle** (`spawn_particle` with the cell's material and a small downward velocity) and clear the cell. Particles die/resettle via the existing particle system (they land as inert matter — do NOT re-root). Call this from `carve_crater` and from the fire burnout path whenever a Mycelium/MushroomFlesh cell is removed. Budget the BFS (cap visited cells; if a group exceeds the budget, treat as supported — err toward not-dropping to stay cheap). Anchors = Rock, Soil (terrain).

- [ ] Tests: `severed_mycelium_bridge_falls` (build a mycelium strand bridging from soil across empty to a floating tip; carve the middle; the disconnected far piece drops — those cells become Empty and particles appear/fall), `anchored_mycelium_stays` (carving near still-anchored mycelium leaves it in place), `settle_after_drop` (after the drop resolves, world sleeps). Verify fail-pre/pass-post. Commit: `feat: unsupported mycelium/mushrooms detach and fall as particles (M1e task 5)`.

---

### Task 6: Switchover — worldgen seeds colonies; rip out old model

**Files:** Modify `worldgen.rs` (seed colony origins), `world.rs` (remove old fields/step-hook/paint), `mycelium.rs`/`growth.rs` (delete old code), `params.rs` (+ json/js) (delete old params), `crates/sandgun-wasm/src/lib.rs`, delete `crates/sandgun-core/tests/growth.rs` old tests.

**Interfaces:** Worldgen: after baking soil richness, spawn `P_MY_WORLDGEN_COLONIES` colonies at random points sitting on/adjacent to soil (each `spawn_colony`), replacing the old pre-filled mycelium veins/groves seeding. Delete the entire old-growth surface listed in "What is KEPT vs REMOVED" (frontier/colonize/reseed/cap-puff/old-grow/old-try_fruit/old params + fields + the old `grow()` step hook). Ensure `step()` calls only `grow_mycelium()` (+ the kept `grow_mushrooms` reveal + decay). Remove `growth.rs` if now empty, or keep only the retained mushroom-shape helpers (move them into `mycelium.rs` and delete `growth.rs`). Update `sandgun-wasm`: remove `frontier_count`/`frontier_drops`; keep `mushroom_count`; add `colony_count`/`tip_count` passthroughs.

- [ ] Delete old growth tests (`tests/growth.rs` frontier/colonize/reseed/maturity/cap tests); keep any mushroom-shape tests by moving them to `tests/mycelium.rs` and adapting to the new fruiting trigger. Verify: `cargo test -p sandgun-core` green (only new-model tests), no references to removed symbols (`grep -rn "frontier\|seed_frontier\|colonize_from\|FrontierCell" crates/` returns nothing). `./scripts/build-wasm.sh` succeeds. A generated world has living colonies (colony_count > 0) that grow. Commit: `refactor: rip out M1c frontier growth; worldgen seeds colonies — new model is the only growth (M1e task 6)`.

---

### Task 7: WASM/HUD + combined chunk-sleep guard + acceptance

**Files:** Modify `crates/sandgun-wasm/src/lib.rs` (colony/tip counts), `web/src/overlay.js` (HUD), `tests/mycelium.rs` (combined guard).

- [ ] **Combined chunk-sleep guard** `full_lifecycle_world_still_sleeps_after_settling`: a generated (or hand-built) world with colonies + avatar + a projectile + particles all live; step until everything settles; assert `tip_count()==0` (or all colonies dead/dormant), `mushroom_len()==0`, and `cells_processed==0`. Do NOT weaken — if it won't settle, report BLOCKED with the trace.
- [ ] **HUD**: overlay.js debug line shows `colonies N · tips M · pool <max colony pool>` (via new wasm passthroughs). Keep it behind `input.debug`.
- [ ] **Browser acceptance (the M1e v1 kill criterion)**: `cd web && npm run dev`; drive headless (Playwright at `/Users/lex/.npm/_npx/e41f203b7505f1fb/node_modules/playwright`, CJS import) OR give a manual checklist. Verify: (1) on load, colonies exist and thin strands visibly creep toward richer soil (not a filling blob); (2) a colony fruits a mushroom after feeding, and not constantly; (3) shooting away a strand's support makes the disconnected part fall; (4) growth settles → `cells_processed` returns to 0 (world sleeps); (5) ~60fps, no console errors. Capture observations. The kill test: watching a colony grow/feed/fruit/wither is compelling AND the world sleeps.
- [ ] Commit: `feat: colony/tip HUD, lifecycle sleep guard — M1e v1 acceptance (task 7)`.

---

## Self-review notes

- Spec coverage: hybrid cells+structs ✓ (T1); explicit colony w/ pool ✓ (T1/T2); per-cell substrate richness ✓ (T1/T2); gradient seek ✓ (T2); branching capped (food + periodic) ✓ (T3); starvation tip-dieback ✓ (T3); nutrient-budget fruiting spending pool at a fed tip ✓ (T4); mushroom decay (simple v1) ✓ (T4); true anchor-connectivity drop via carve-flood ✓ (T5); worldgen seeds colonies + baseline richness ✓ (T1/T6); rip-out old model ✓ (T6); chunk-sleep guard ✓ (T7).
- **DEVIATION from spec, flagged for Lex:** the spec said "union-find maintained incrementally + carve-reflood"; this plan uses **flood-on-carve only** (no persistent union-find) + **colony id in mycelium aux** (no cross-colony merge in v1). Same drop-correctness, less machinery; colony *merging* deferred to v2. If Lex wants incremental union-find/merge in v1, T5 changes.
- **v2 deferred (not in this plan):** water-seep + ash enrichment (v1 richness is static worldgen baseline), mushroom-rot→rich-substrate feedback (v1 decay is ash/empty), species-as-color, white mycelium recolor, wider/curved stipes, cohesive-clump physics falling.
- Known accepted v1 simplifications: ≤255 colonies (u8 id); colonies don't merge on contact; carve-flood errs toward "supported" past its budget (won't wrongly drop huge masses).
