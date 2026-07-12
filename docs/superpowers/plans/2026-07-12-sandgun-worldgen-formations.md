# SANDGUN — Structured Worldgen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the per-column-noise worldgen with a **structured-procedural** generator: depth-based biome bands, organic connected cavern systems, a **guaranteed descendable path** top→bottom, and placed set-pieces (soil beds / mushroom groves, liquid pools, material pockets) — so a 1024×2048 world reads as natural (not streaky) and is always traversable, while keeping the M1e soil-richness bake + colony seeding.

**Architecture:** All in `crates/sandgun-core/src/worldgen.rs` (pure Rust, no asset pipeline). `generate(world, seed)` is rewritten as ordered passes: biome bands → base fill → CA caverns → **connectivity carve + verify** → set-pieces (soil beds, liquid pools in basins, material pockets) → richness bake → colony seeding. Determinism via the existing generation-local `GenRng` (the sim's own RNG stream stays untouched). Design decisions from the 2026-07-12 focused design round: structured-procedural (no templates/assets), guaranteed descendable, all four feature families, replace-not-layer.

**Tech Stack:** unchanged. No new deps.

## Global Constraints

- Worldgen runs in `sandgun-core` only, via `GenRng` (never `World::next_rand` — the sim RNG stream must stay independent so worldgen changes don't shift live-sim determinism). Cell stays 4 bytes; aux semantics unchanged (Soil aux = richness, set in the bake pass; Mycelium aux = colony id, set by `spawn_colony`).
- World dims are multiples of `CHUNK=64`; target 1024×2048. Must not assume a specific size — read `world.width`/`world.height`.
- Chunk sleeping SACRED: after generation, `world.wake_all()` is called (as today) so the world settles alive; the generated world must reach `cells_processed==0` (settle) — no perpetual churn from bad geometry (e.g. floating powders that never rest). Keep colony seeding so growth starts, but a generated world must still eventually settle.
- **Keep**: `world.set_soil_richness` bake pass + `spawn_colony` seeding (M1e), `world.wake_all()`, the `GenRng`/`set`/`blob` helpers (extend as needed).
- Determinism: same seed → identical world. All randomness via `GenRng`.
- Commits end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` (second `-m`). Run cargo from repo root; rebuild wasm with `./scripts/build-wasm.sh`.
- **Branch:** `git checkout -b worldgen-formations` before Task 1.

## Current state (what's replaced)

`worldgen.rs::generate` today: surface heightline → fill rock below → CA cave carve → soil crust → soil-heavy CA (most rock→soil) → spore/sand/water/oil blobs → rock declump → wake_all → richness bake → colony seed. It reads "streaky/uniform" at 2048 tall (no depth variety; soil-CA + declump produce column-ish masses; liquids float as random blobs rather than pooling). This plan replaces passes 1–4b with a banded, cavern-first, connectivity-guaranteed structure; keeps passes 5–7 (wake/bake/seed).

---

### Task 1: Biome bands + base fill + surface

**Files:** Modify `crates/sandgun-core/src/worldgen.rs`; add tests in `crates/sandgun-core/tests/worldgen.rs`.

**Interfaces produced:** a `Biome` descriptor (depth range + base material mix + cave density + feature weights) and a `fn biomes(h) -> Vec<Biome>` that partitions the world height into 3–4 depth bands. `generate` fills below the surface heightline with each band's base material mix (soil-leaning near the top, rock-leaning deep), instead of uniform rock + a global soil-CA.

- [ ] **Step 1: Failing tests** (`tests/worldgen.rs`):
```rust
use sandgun_core::cell::Material;
use sandgun_core::world::World;

fn count(w: &World, m: Material) -> usize {
    let mut n = 0;
    for y in 0..w.height { for x in 0..w.width { if w.get(x,y)==m { n+=1; } } }
    n
}

#[test]
fn world_has_open_sky_above_the_surface() {
    let mut w = World::new(256, 512);
    w.generate(1);
    // the very top row is open (above the surface heightline)
    let top_empty = (0..w.width).filter(|&x| w.get(x, 0) == Material::Empty).count();
    assert!(top_empty > w.width * 3 / 4, "top row should be mostly open sky");
}

#[test]
fn deeper_band_is_rockier_than_the_upper_band() {
    // soil should dominate near the surface, rock deeper — a depth gradient, not uniform fill.
    let mut w = World::new(256, 512);
    w.generate(2);
    let band = w.height / 4;
    let upper_rows = band..band*2;      // upper subsurface band
    let deep_rows = band*3..band*4;     // deep band
    let ratio = |rows: std::ops::Range<usize>| {
        let (mut soil, mut rock) = (0usize, 0usize);
        for y in rows { for x in 0..w.width {
            match w.get(x,y) { Material::Soil => soil+=1, Material::Rock => rock+=1, _=>{} }
        }}
        (soil, rock)
    };
    let (us, ur) = ratio(upper_rows);
    let (ds, dr) = ratio(deep_rows);
    // upper band is soil-leaning, deep band is rock-leaning
    assert!(us as f32 / (ur.max(1) as f32) > ds as f32 / (dr.max(1) as f32),
        "soil:rock ratio must decrease with depth (upper {us}:{ur} vs deep {ds}:{dr})");
}
```

- [ ] **Step 2: Run to verify fail** — `cargo test -p sandgun-core --test worldgen` → the depth-gradient test fails against the current uniform soil-CA.

- [ ] **Step 3: Implement bands + fill.** In `worldgen.rs`, add:
```rust
struct Biome {
    top: usize,          // inclusive world row where this band starts
    soil_pct: u32,       // 0..100 chance a filled cell is Soil vs Rock (before caves)
    cave_seed_pct: u32,  // 0..100 initial open-cell chance for the CA carve (Task 2)
}
/// Partition [surface .. h) into depth bands: soil-rich crust zone near the top grading to
/// rock-dominant depths. Boundaries scale with world height so it works at any size.
fn biomes(h: usize) -> Vec<Biome> {
    vec![
        Biome { top: 0,          soil_pct: 78, cave_seed_pct: 42 }, // upper: soft, soil-rich
        Biome { top: h * 2 / 5,  soil_pct: 45, cave_seed_pct: 46 }, // mid: mixed
        Biome { top: h * 7 / 10, soil_pct: 20, cave_seed_pct: 40 }, // deep: rock-dominant
    ]
}
fn biome_at<'a>(bands: &'a [Biome], y: usize) -> &'a Biome {
    bands.iter().rev().find(|b| y >= b.top).unwrap_or(&bands[0])
}
```
Rewrite the fill (replace current passes 2, 3b, 3c): keep the surface heightline (pass 1). Below `surface[x]`, fill each cell with the band's base material by a per-cell `GenRng` roll: `if rng.chance(band.soil_pct, 100) { Soil } else { Rock }`. (Caves carved in Task 2; soil beds refined in Task 3. Do NOT do the old global soil-CA or declump — the banded fill + Task 3 soil beds replace them.) A short smoothing CA pass over the soil/rock fill (majority rule, 1–2 iterations) removes salt-and-pepper so bands read as cohesive masses, not noise.

- [ ] **Step 4: Run tests pass** — `cargo test -p sandgun-core --test worldgen`. Then full `cargo test -p sandgun-core` (colony/settle tests still pass — richness bake + seeding still run at the end).

- [ ] **Step 5: Commit** — `feat: worldgen biome bands + depth-graded base fill (worldgen task 1)`

---

### Task 2: Connected caverns + guaranteed descendable path

**Files:** Modify `worldgen.rs`; tests in `tests/worldgen.rs`.

**Interfaces produced:** CA cavern carve (per-biome density, isotropic — no vertical streak bias), then a **connectivity pass**: flood-fill Empty from the surface opening; if the bottom row isn't reached, carve a meandering tunnel connecting the surface-reachable region down to the bottom, guaranteeing a descendable path. `fn descendable(world) -> bool` logic (surface→bottom flood) is exercised by a test.

- [ ] **Step 1: Failing test** (`tests/worldgen.rs`):
```rust
// A path of Empty cells connects the top surface to the bottom of the world (you can descend).
fn descendable(w: &World) -> bool {
    use std::collections::VecDeque;
    let (wd, ht) = (w.width, w.height);
    let mut seen = vec![false; wd*ht];
    let mut q = VecDeque::new();
    // seed from every Empty cell in the top row (open sky)
    for x in 0..wd { if w.get(x,0)==Material::Empty { seen[x]=true; q.push_back((x,0)); } }
    while let Some((x,y)) = q.pop_front() {
        if y == ht-1 { return true; } // reached the bottom
        for (nx,ny) in [(x as i32-1,y as i32),(x as i32+1,y as i32),(x as i32,y as i32-1),(x as i32,y as i32+1)] {
            if nx<0||ny<0||nx as usize>=wd||ny as usize>=ht { continue; }
            let (nx,ny)=(nx as usize,ny as usize);
            if !seen[ny*wd+nx] && w.get(nx,ny)==Material::Empty {
                seen[ny*wd+nx]=true; q.push_back((nx,ny));
            }
        }
    }
    false
}

#[test]
fn every_generated_world_is_descendable() {
    for seed in 1..=12u32 {
        let mut w = World::new(256, 512);
        w.generate(seed);
        assert!(descendable(&w), "seed {seed} produced a non-descendable world");
    }
}
```
(Note: `descendable` walks Empty only — worldgen liquids in pools may sit in the path; run this connectivity check BEFORE placing liquids in Task 3, OR have the check treat shallow liquid as passable. Simplest: guarantee + test the Empty-connectivity in Task 2 before Task 3 adds liquids into some of that Empty; Task 3 must not fully seal the descent — keep pools shallow / in side basins. Add a Task-3 re-check.)

- [ ] **Step 2: Run to verify fail** (some seeds likely fail without a connectivity guarantee).

- [ ] **Step 3: Implement caverns + connectivity.** Carve CA caverns (adapt the current pass 3 CA, seeded per-biome `cave_seed_pct`, 4 iterations, isotropic 8-neighbor majority — verify no axis bias). Then the connectivity carve:
```rust
// Flood Empty from the surface; if the bottom isn't reached, carve a meandering shaft from the
// deepest surface-reachable open cell down to the bottom (a few cells wide), so the world is
// always descendable. Caves then read as branches off this guaranteed route.
fn ensure_descendable(world: &mut World, rng: &mut GenRng) {
    // 1. BFS Empty from the top row; record reached cells + the deepest reached (x,y).
    // 2. if the bottom row was reached, done.
    // 3. else, from the deepest reached open cell, carve a random-walk tunnel downward
    //    (bias down, jitter x by rng.range(-1,2), width 2-3) to the bottom row, setting Empty.
}
```
Implement the BFS + carve (full code — mirror the `descendable` BFS, track deepest reached, then a downward random-walk carve clamped to bounds). Call `ensure_descendable` after the cavern carve. Keep the soil crust (a thin colonizable skin along cavern/surface edges) — but the deep soil is now the banded fill from Task 1, not a global CA.

- [ ] **Step 4: Run tests pass** — the 12-seed descendable test passes; full suite green.

- [ ] **Step 5: Commit** — `feat: connected CA caverns + guaranteed descendable path (worldgen task 2)`

---

### Task 3: Set-pieces — soil beds/groves, liquid pools, material pockets

**Files:** Modify `worldgen.rs`; tests in `tests/worldgen.rs`.

**Interfaces produced:** placed features in the carved caverns: **soil beds / mushroom groves** (fertile Soil patches on cavern floors where colonies seed), **liquid pools** (water/oil/acid pooled in cavern *basins* — local low points — not floating blobs), **material pockets** (sand, spore gas). Colony seeding (kept) prefers the soil beds. Re-verify descendability after (pools must not seal the descent).

- [ ] **Step 1: Failing tests** (`tests/worldgen.rs`):
```rust
#[test]
fn world_has_soil_beds_and_liquids_and_stays_descendable() {
    let mut w = World::new(256, 512);
    w.generate(3);
    assert!(count(&w, Material::Soil) > 500, "should have substantial soil beds");
    let liquid = count(&w, Material::Water) + count(&w, Material::Oil) + count(&w, Material::Acid);
    assert!(liquid > 0, "should place some liquid pools");
    assert!(descendable(&w), "features must not seal the descent"); // reuse the Task 2 helper
}

#[test]
fn colonies_are_seeded_on_soil() {
    let mut w = World::new(256, 512);
    w.generate(4);
    assert!(w.colony_count() > 0, "worldgen should seed colonies");
}
```

- [ ] **Step 2: Run to verify fail** (soil-beds/liquid-pool placement not yet structured).

- [ ] **Step 3: Implement set-pieces.** Replace the old blob-scatter (passes 3d, 4):
  - **Soil beds / groves:** on cavern *floors* (an Empty cell with solid below), stamp soil patches (a few cells of Soil into the wall/floor) at chosen sites — designated fertile zones, biome-weighted (more in the upper band). These are where colonies seed.
  - **Liquid pools:** find cavern *basins* — Empty cells whose downward neighbors are solid (a floor) forming a local low point — and pour a liquid (water upper, oil mid, acid rare/deep) that pools there rather than a floating blob. Keep pools shallow / in side basins so they don't seal the main descent.
  - **Material pockets:** sand dunes near the surface, spore-gas blobs in cave air (keep, lightly).
  - Keep the richness bake (pass 6) and colony seeding (pass 7), but bias colony origins toward the soil beds (`soil_sites` already collected in the bake pass — seed from those).
  - After placement, if a Task-2-style descendability check now fails (a pool sealed the route), carve a bypass or drain — simplest: run `ensure_descendable` again after features. (The test enforces this.)

- [ ] **Step 4: Run tests pass**; full suite green (settle/chunk-sleep tests still pass — pooled liquids settle, don't churn forever).

- [ ] **Step 5: Commit** — `feat: worldgen set-pieces — soil beds, basin liquid pools, pockets; colonies seed on beds (worldgen task 3)`

---

### Task 4: Settle/perf + acceptance

**Files:** `tests/worldgen.rs`; browser acceptance.

- [ ] **Step 1: Settle guard test.** `generated_world_settles_and_sleeps`: generate a 256×512 (or the existing test-world size) world, step enough that everything settles (generous budget), assert `cells_processed == 0` after a final step (no perpetual churn from bad geometry — floating powders, unstable pools). If it never settles, that's a real defect (diagnose geometry). Do NOT weaken.

- [ ] **Step 2: Browser acceptance (natural look + descend + 60fps).** `./scripts/build-wasm.sh`; `cd web && npm run dev`. Drive headless (Playwright) or document a manual checklist. Verify at the real 1024×2048: (1) the terrain reads NATURAL — cohesive biome bands, organic caverns, NO vertical streaks / salt-and-pepper (screenshot several seeds, eyeball / check that no column of the world is a 1-wide alternating pattern); (2) you can descend top→bottom (the connectivity guarantee holds at full size — flood-fill check via a headless probe or by walking the avatar down); (3) soil beds host growing colonies, liquid pools sit in basins; (4) ~60fps on the descent (real-Mac confirmation from Lex if headless is unreliable); (5) no console errors. Capture screenshots (Read-able PNGs) of 2–3 seeds for the report.

- [ ] **Step 3: Commit** — `feat: worldgen settle guard + acceptance (worldgen task 4)`

---

## Self-review notes

- Design coverage: structured-procedural (no assets) ✓; biome bands ✓ (T1); connected caverns + guaranteed descendable ✓ (T2); soil beds/groves + liquid pools + pockets ✓ (T3); replace-not-layer ✓ (T1 replaces the fill/soil-CA/declump); keep richness+seeding ✓ (T3); natural + descendable + settles + 60fps kill criterion ✓ (T4).
- Determinism: all worldgen randomness via `GenRng` (not the sim RNG) — worldgen changes never shift live-sim determinism.
- Ordering subtlety flagged: descendability is checked/guaranteed on EMPTY connectivity in T2 (before liquids), then re-verified after T3's pools so a pool can't seal the descent.
- Chunk-sleep: T4's settle guard ensures no geometry causes perpetual churn (the world must reach cells_processed==0).
- Deferred (not this pass): hand-authored template scenes / asset pipeline, wang-tile stitching, ore/wood materials, weather. This is the structured-procedural pass; templates remain a possible future milestone.
