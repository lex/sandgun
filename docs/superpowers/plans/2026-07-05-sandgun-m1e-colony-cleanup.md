# SANDGUN M1e — Colony Lifecycle Cleanup Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the colony-lifecycle issues found in the M1e code read (2026-07-05): unbounded `colonies` Vec growth ("zombie" colonies), per-tick cost that scales with it, and colony-id wrap at 255 — plus three smaller correctness/perf/feel nits (acid/burn flood coalescing, recede-through-fire aux, spore-ammo-in-air).

**Architecture:** All in `crates/sandgun-core/src/mycelium.rs` + `world.rs`. The root fix is to make colonies **reapable**: a colony that has no live tips AND no live cells is removed from the `colonies` Vec and its `u8` id returned to a free-list for reuse. Cell ownership is tracked with a per-colony `cell_count` so an id is only recycled once none of its mycelium cells remain (otherwise a reused id would mislabel orphaned cells). This bounds the Vec, bounds the per-tick `.find` cost, and makes the 255-id space concurrent-not-cumulative.

**Tech Stack:** unchanged (Rust stable, wasm-bindgen/wasm-pack, Vite). No new deps.

## Global Constraints

- Cell stays 4 bytes; flags bit 7 reserved; NO per-cell temperature. **aux semantics unchanged:** Soil aux = substrate richness, Mycelium aux = colony id (1–255; 0 = none), burning aux = fuel. A colony id of 0 is never valid.
- Chunk sleeping is SACRED: none of these changes may add per-frame work on an idle world. Reaping runs only inside `grow_mycelium` (already cadence-gated) and touches only the bounded colony list. Every cell mutation still `wake()`s.
- Determinism: any new randomness via `next_rand`/`chance` only (none expected here).
- Sim logic only in `sandgun-core`; `sandgun-wasm` glue-only. `colony_count()`/`tip_count()`/`max_colony_pool()` wasm getters keep working.
- All commits end with: `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` (second `-m`). Run cargo from repo root; rebuild wasm with `./scripts/build-wasm.sh`.
- **Branch:** these fix M1e code that is not yet merged — work on `m1e-living-mycelium` (current tip). No new branch.

## Current state (what's being fixed)

- `World.colonies: Vec<Colony>`; `spawn_colony` (mycelium.rs:66) sets `id = colonies.len() as u8 + 1` and never removes colonies. `colony_pool`/`colony_starving`/`extend_tip`/`thicken_strand` all `.find(|c| c.id == id)` linearly.
- A colony is marked `alive=false` only when `tip_count==0 && nutrient_pool==0` (mycelium.rs:172-176). Because fruiting leaves `pool ≈ threshold - cost > 0`, a colony that fruits then loses its tips stays `alive` forever ("zombie") — Vec never shrinks, ids climb toward the 255 wrap in `spawn_colony`.

---

### Task 1: Reap colonies + id free-list + per-colony cell_count

**Files:**
- Modify: `crates/sandgun-core/src/mycelium.rs` (Colony field, spawn/lay/remove accounting, reap pass, id allocation)
- Modify: `crates/sandgun-core/src/world.rs` (new `free_colony_ids` field + init + clear; decrement on cell-removal paths)
- Modify: `crates/sandgun-core/tests/mycelium.rs` (tests)

**Interfaces produced:**
- `Colony` gains `cell_count: u32` (live Mycelium cells owned by this colony).
- `World.free_colony_ids: Vec<u8>` (recycled ids, LIFO).
- `World::alloc_colony_id() -> Option<u8>` — pop a free id, else the next unused id up to 255, else `None` (at the concurrent cap).
- `World::colony_cell_laid(id)` / `World::colony_cell_removed(id)` — inc/dec a colony's `cell_count` (called from every lay/remove site).
- `spawn_colony` returns `u8` still, but returns `0` (no colony) if `alloc_colony_id()` is `None` (caller must tolerate 0 — see Task 2 for the Spore-ammo caller).

- [ ] **Step 1: Write failing tests** (append to `crates/sandgun-core/tests/mycelium.rs`):
```rust
#[test]
fn tipless_colony_is_reaped_not_zombied() {
    let mut w = World::new(64, 64);
    // colony in open air with no soil: grows a stub, tips die at the air-reach cap, then it
    // has no tips. Even if it fruited nothing, it must be reaped (removed), not left alive.
    let id = w.spawn_colony(32, 32);
    for _ in 0..2000 { w.step(); }
    assert_eq!(w.colony_pool(id), 0, "reaped/absent colony reports pool 0");
    assert_eq!(w.colony_count(), 0, "a colony with no live tips and no live cells is reaped");
}

#[test]
fn colony_id_is_recycled_after_reap() {
    let mut w = World::new(64, 64);
    let a = w.spawn_colony(10, 10);
    for _ in 0..2000 { w.step(); } // a's tips die in open air, its stub recedes/reaps
    let b = w.spawn_colony(50, 50);
    assert_eq!(a, b, "the reaped colony's id is reused for the next colony");
}

#[test]
fn colony_with_live_cells_keeps_its_id_until_cells_gone() {
    // A colony that still has mycelium cells in the grid must NOT have its id reused, or a new
    // colony would inherit ownership of the old cells.
    let mut w = World::new(96, 96);
    for x in 0..96 { for y in 60..70 { w.paint(x as i32, y as i32, 0, Material::Soil as u8); w.set_soil_richness(x, y, 200); } }
    let a = w.spawn_colony(48, 65);
    for _ in 0..400 { w.step(); } // a grows a real network in the soil
    // a is alive with cells; the next spawn must get a DIFFERENT id
    let b = w.spawn_colony(10, 65);
    assert_ne!(a, b, "an id with live cells is not recycled");
}
```

- [ ] **Step 2: Run to verify they fail** — `cargo test -p sandgun-core --test mycelium` → the reap/recycle tests fail (colonies currently never removed; id is `len+1`).

- [ ] **Step 3: Add `cell_count` + free-list + id allocation.**

In `mycelium.rs`, add to `Colony`:
```rust
    /// Live Mycelium cells in the grid owned by this colony (aux == id). A colony is only reaped
    /// (and its id recycled) once this reaches 0, so a recycled id can never mislabel orphan cells.
    pub cell_count: u32,
```
Init it to 1 in `spawn_colony` (the root cell), and replace the id line + push:
```rust
    pub fn spawn_colony(&mut self, x: usize, y: usize) -> u8 {
        let Some(id) = self.alloc_colony_id() else { return 0; };
        self.colonies.push(Colony { id, nutrient_pool: 0, tip_count: 1, alive: true, age_ticks: 0, cell_count: 1 });
        let i = self.idx(x, y);
        self.cells[i].material = Material::Mycelium as u8;
        self.cells[i].aux = id;
        self.cells[i].flags &= !crate::cell::FLAG_BURNING;
        self.wake(x, y);
        self.tips.push(Tip { x, y, colony: id, last_dx: 0, last_dy: -1, alive: true, air_run: 0 });
        id
    }

    /// Allocate a colony id: reuse a freed one, else the next unused id (1..=255). None at the cap.
    fn alloc_colony_id(&mut self) -> Option<u8> {
        if let Some(id) = self.free_colony_ids.pop() { return Some(id); }
        // ids in use are exactly those in `colonies` (reaped ones went to the free-list). Next id
        // is len+1 while under 255; the concurrent cap is 255 (aux is u8, 0 reserved for "none").
        let next = self.colonies.len() + 1;
        if next <= 255 { Some(next as u8) } else { None }
    }

    /// Account a Mycelium cell newly laid for `id`.
    pub(crate) fn colony_cell_laid(&mut self, id: u8) {
        if let Some(c) = self.colonies.iter_mut().find(|c| c.id == id) { c.cell_count = c.cell_count.saturating_add(1); }
    }
    /// Account a Mycelium cell of `id` removed from the grid (reverted/carved/burned/dissolved/dropped).
    pub(crate) fn colony_cell_removed(&mut self, id: u8) {
        if let Some(c) = self.colonies.iter_mut().find(|c| c.id == id) { c.cell_count = c.cell_count.saturating_sub(1); }
    }
```
Add the field in `world.rs`: `pub(crate) free_colony_ids: Vec<u8>,` — init `Vec::new()` in `new()`, and in `clear()` add `self.free_colony_ids.clear();`.

- [ ] **Step 4: Count every lay and every removal.**

- Lay sites (increment): in `extend_tip` after setting the tip's cell to Mycelium, call `self.colony_cell_laid(t.colony);`. In `thicken_strand`'s write loop, after laying each extra cell, `self.colony_cell_laid(colony_id);`. (spawn_colony already sets `cell_count: 1`.)
- Removal sites (decrement) — every place a **Mycelium** cell leaves the grid must read its colony id (`aux`) *before* clearing and decrement:
  - `recede_tip` (mycelium.rs:338): before `self.cells[i].material = Empty`, `let cid = self.cells[i].aux; ... self.colony_cell_removed(cid);` (guard: only when the cell was Mycelium, which it checks).
  - `flood_group_and_maybe_drop` drop loop (mycelium.rs:522): for each dropped cell, if `mat == Mycelium as u8`, `self.colony_cell_removed(<aux of that cell>)` — read aux before `Cell::default()`.
  - `world.rs` removal paths that can clear a Mycelium cell: `carve_crater`, `update_burning` burnout (mycelium→ash/empty), `update_acid` dissolve. At each, if the cell being cleared is `Material::Mycelium`, read its `aux` and call `self.colony_cell_removed(aux)` before overwriting. (Search these functions; add the decrement next to the existing clear.)
  - NOTE: mushroom flesh has aux=0 (not a colony) — only decrement for `Mycelium` cells, never `MushroomFlesh`.

- [ ] **Step 5: Reap dead colonies + recycle ids.** Replace the colony-death pass in `grow_mycelium` (mycelium.rs:172-176) with:
```rust
        // A colony with no live tips is functionally dead (nothing left to grow or fruit from).
        // Zero any leftover pool so it can't linger as a "zombie", then reap it once none of its
        // cells remain in the grid -- freeing its id for reuse (an id with live cells is NOT
        // recycled, so a new colony can never inherit old cells).
        for c in self.colonies.iter_mut() {
            if c.tip_count == 0 { c.alive = false; c.nutrient_pool = 0; }
        }
        let mut freed: Vec<u8> = Vec::new();
        self.colonies.retain(|c| {
            let reap = !c.alive && c.cell_count == 0;
            if reap { freed.push(c.id); }
            !reap
        });
        self.free_colony_ids.extend(freed);
```
(Keep this AFTER the `tip_count` recompute so `tip_count==0` is accurate, and BEFORE `fruit_fed_colonies`.)

- [ ] **Step 6: Run tests** — `cargo test -p sandgun-core --test mycelium` (3 new pass) then full `cargo test -p sandgun-core` (existing green — the combined chunk-sleep guard + generated-world-settles must still pass; a reaped-empty colony list is fine). `./scripts/build-wasm.sh` succeeds.

- [ ] **Step 7: Commit** — `git add -A && git commit -m "fix: reap tipless/cell-less colonies, recycle ids via free-list + cell_count (M1e cleanup task 1)"`

---

### Task 2: Concurrent-colony cap + Spore-ammo tolerates a full colony table

**Files:** Modify `crates/sandgun-core/src/world.rs` (`Ammo::Spore` on_impact), `crates/sandgun-core/tests/` (test).

**Interfaces:** `spawn_colony` now returns `0` when `alloc_colony_id()` is at the 255 cap (Task 1). Callers must tolerate `0` (= "no colony spawned"). Optionally add a lower soft cap `P_MY_MAX_COLONIES` (default e.g. 64) so the table stays small; `alloc_colony_id` returns `None` at that soft cap instead of 255.

- [ ] **Step 1:** Add param `P_MY_MAX_COLONIES` (default 64), triple-mirrored (params.rs index + P_COUNT, params.json, params.js — verify counts agree). In `alloc_colony_id`, cap `next`/free-list use at `min(255, P_MY_MAX_COLONIES)` concurrent live colonies.
- [ ] **Step 2:** In `Ammo::Spore`'s `on_impact` handler (world.rs ~469), it currently does `self.spawn_colony(cx, cy)`. Guard it: if the return is `0`, do nothing (or fall back to injecting inert mycelium) — do NOT create a tip referencing a non-existent colony 0. Also (feel fix folded in): prefer a Soil-adjacent cell near impact so a spore round fired into open air plants on substrate rather than a floating stub (search the small neighborhood of the impact for a Soil or Soil-adjacent Empty cell; if none, skip). This also resolves the "spore ammo in open air makes a floating blob" nit.
- [ ] **Step 3:** Test `spore_ammo_at_colony_cap_is_a_noop`: set `P_MY_MAX_COLONIES` low (e.g. 2), spawn to the cap, fire Spore ammo, assert `colony_count()` never exceeds the cap and no panic / no colony-0 tip. And `spore_ammo_plants_on_substrate_not_air`: fire Spore into open air above soil, assert the resulting colony (if any) sits on/adjacent to Soil, not floating. Verify fail-pre where applicable / pass-post.
- [ ] **Step 4:** Full suite + wasm build green. Commit: `fix: cap concurrent colonies; spore ammo plants on substrate, tolerates full table (M1e cleanup task 2)`

---

### Task 3: Coalesce acid/burn support-floods to one pass per step

**Files:** Modify `crates/sandgun-core/src/world.rs` (defer drop checks), `crates/sandgun-core/src/mycelium.rs` (batch entry point), test.

**Why:** `drop_unsupported_around` is currently called inline from `update_burning` and `update_acid` — once per burned/dissolved Mycelium/MushroomFlesh cell, *during* the sweep. A large acid pool or fire eating a large mycelium mass fires many bounded floods per frame (perf cliff; also redundant — overlapping regions re-flood the same cells). This is a perf/cleanliness fix, not correctness.

**Interfaces:** `World.pending_drop_checks: Vec<(isize, isize)>` — removal sites push their (x,y) here instead of calling `drop_unsupported_around` inline. After the cell sweep in `step()` (before/after growth — pick one and document), run a single coalesced pass: `drop_unsupported_pending()` that seeds ONE flood over the union of pending regions with a shared `visited` set, so each connected group is checked at most once per step.

- [ ] **Step 1:** Add `pending_drop_checks: Vec<(isize,isize)>` (init + clear in `clear()`). Change the burnout (world.rs:788) and acid-dissolve (world.rs:901) call sites from `self.drop_unsupported_around(...)` to `self.pending_drop_checks.push((x, y));`. (Leave `carve_crater`/`inject_blob` — those are one-shot player events, not sweep-driven; either route is fine, but routing them through pending too is cleaner and dedups when a kinetic blast overlaps mycelium.)
- [ ] **Step 2:** Add `World::drop_unsupported_pending(&mut self)`: if `pending_drop_checks` is empty, return (chunk-sleep safe). Build ONE `visited: HashSet`, then for each pending (x,y) and each Mycelium/MushroomFlesh seed within a small radius not yet visited, call `flood_group_and_maybe_drop` sharing that `visited`. Clear `pending_drop_checks` at the end. Call it once in `step()` right after the cell sweep (and after grow_mycelium's own carves? — document: call it after grow_mycelium so growth-tick removals are also covered, OR keep growth removals routed through pending too).
- [ ] **Step 3:** Test `overlapping_acid_removals_flood_each_group_once`: instrument or assert behaviorally that a mycelium mass severed by acid still drops correctly (equivalent outcome to the inline version — reuse the existing `acid_ammo_severs_mycelium_bridge_and_far_side_falls` scenario, which must still pass) AND that a settled world with pending cleared sleeps (cells_processed==0). The perf win itself needn't be unit-tested; correctness-equivalence + chunk-sleep is what matters.
- [ ] **Step 4:** Full suite + wasm build green (all existing drop tests must still pass unchanged in outcome). Commit: `perf: coalesce acid/burn support-floods into one deduped pass per step (M1e cleanup task 3)`

---

### Task 4: Recede through a burning strand (aux-collision fix)

**Files:** Modify `crates/sandgun-core/src/mycelium.rs` (`adjacent_same_colony_mycelium` + degree), test.

**Why:** A burning Mycelium cell's `aux` holds fire fuel, not the colony id, so `recede_tip`'s `adjacent_same_colony_mycelium` (mycelium.rs:379) treats a burning same-strand neighbor as foreign. A tip receding through a strand whose next segment is on fire dies early and strands the rest as a permanent stub.

- [ ] **Step 1:** In `adjacent_same_colony_mycelium` and `same_colony_mycelium_degree`, treat a neighbor as same-colony mycelium when it is `Material::Mycelium` AND (`aux == colony_id` OR `flags & FLAG_BURNING != 0`). Rationale: a burning Mycelium cell adjacent to a receding same-colony strand is overwhelmingly part of that strand (growth never lays foreign mycelium adjacent), and it will self-remove via burnout anyway — treating it as traversable lets the recede continue past it instead of orphaning the tail. (Import `FLAG_BURNING` in mycelium.rs if not already.)
- [ ] **Step 2:** Test `recede_continues_past_a_burning_segment`: build a straight mycelium strand, set one middle cell burning (paint Fire adjacent + step until it ignites, or set FLAG_BURNING via the existing test approach), force the colony to starve, and assert the tip recedes PAST the burning cell (cells beyond it get reverted; no permanent stub left once the fire also burns out). Verify fail-pre (tip dies at the burning cell, stub remains) / pass-post.
- [ ] **Step 3:** Full suite + wasm build green (chunk-sleep/termination unaffected — the walk still strictly shrinks). Commit: `fix: recede walks through a burning same-colony segment instead of stranding it (M1e cleanup task 4)`

---

## Self-review notes

- Root fix (#1/#2/#3) is Task 1: `cell_count` + free-list makes reaping safe (no orphan-id mislabel), bounds the Vec (kills per-tick `.find` creep), and makes the 255 id space concurrent-not-cumulative. Task 2 adds a hard concurrent cap as a backstop + fixes the spore-in-air feel nit.
- Tasks 3 (flood coalescing) and 4 (recede-through-fire) are the independent minors; either can be dropped/deferred without affecting Task 1/2.
- **The one design tradeoff to confirm with Lex:** the concurrent-colony cap (`P_MY_MAX_COLONIES`, Task 2) means Spore ammo can become a no-op once the world is saturated with colonies. Alternative: reap the *oldest/weakest* colony to make room. Default plan is "no-op at cap" (simplest); flag for Lex if he'd rather Spore always plant by evicting the weakest.
- Chunk-sleep preserved throughout: reaping + coalesced flood + pending list all run only on active work and no-op when empty; determinism untouched (no new RNG).
- Not addressed here (deliberately, separate backlog items): the dead-sub-branch cosmetic stub at tree junctions (needs full graph connectivity), the 8-conn-group vs 4-conn-anchor asymmetry, and the fallen-particle-loses-aux cosmetic — all previously triaged as non-blocking cosmetics.
