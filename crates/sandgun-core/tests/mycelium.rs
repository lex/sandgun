use sandgun_core::cell::Material;
use sandgun_core::projectile::Ammo;
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

#[test]
fn tip_grows_toward_richer_substrate() {
    let mut w = World::new(64, 64);
    // rich soil to the right of the colony, poor/empty to the left. Soil is granular (falls
    // under gravity like sand): an unsupported block free-falls to the bottom of the world
    // within ~30 frames, long before a slow-growing tip could ever reach it, which would make
    // this test just check whether a random walk crosses an arbitrary band of cells rather than
    // whether richness actually attracts the tip. So pin the soil down with a rock floor. Use a
    // single-row soil strip with the floor widened by 1 column on each side: any narrower and
    // the edge grains slump diagonally into the still-open neighboring column and drift toward
    // the colony on their own, reaching it via physics rather than via the tip's own seeking.
    for x in 33..51 { w.paint(x, 33, 0, Material::Rock as u8); }
    for x in 34..50 { w.paint(x as i32, 32, 0, Material::Soil as u8); w.set_soil_richness(x, 32, 200); }
    w.spawn_colony(32, 32);
    // The spawn point is 2 cells shy of the substrate, so the tip senses no gradient on its
    // first move; the momentum bias (see pick_step) then locks it onto a fairly long, mostly
    // straight ray until a wall or the substrate's richness redirects it. With this world's
    // deterministic RNG stream the tip reaches the rich soil at step 229 -- budget generously
    // (1000) rather than tuning to the exact deterministic step count.
    for _ in 0..1000 { w.step(); }
    // mycelium should have advanced rightward into the rich soil
    let reached = (34..50).any(|x| w.get(x, 32) == Material::Mycelium);
    assert!(reached, "tip should creep toward the rich soil");
}

#[test]
fn tip_turns_toward_adjacent_food() {
    let mut w = World::new(64, 64);
    // A fresh tip's initial momentum points straight up (last_dx=0, last_dy=-1; see
    // spawn_colony). Put the one rich cell to the right instead, so momentum cannot explain
    // the tip stepping there -- only richness can. Every other neighbor is left at its default
    // (Empty, richness 0), so there's no other signal competing with the rich cell. Soil is
    // granular, so support it on a row of Rock (not just directly underneath -- a lone support
    // cell still lets the grain slump diagonally into either open neighbor) -- otherwise it
    // falls or slides away under gravity (which runs before growth within the same step()) before
    // the tip ever gets a chance to sense it.
    for x in 32..=34 { w.paint(x, 33, 0, Material::Rock as u8); }
    w.paint(33, 32, 0, Material::Soil as u8);
    w.set_soil_richness(33, 32, 200);
    let id = w.spawn_colony(32, 32);
    let before = w.colony_pool(id);
    // my_grow_countdown starts at 0, so a single world step is exactly one growth tick.
    w.step();
    assert_eq!(
        w.get(33, 32),
        Material::Mycelium,
        "tip should step directly into the adjacent rich soil rather than follow momentum"
    );
    assert!(w.colony_pool(id) > before, "stepping into rich soil should fill the pool");
}

#[test]
fn tip_count_tracks_live_tips() {
    let mut w = World::new(64, 64);
    // Box the spawn point in on all 8 sides with Rock, so the tip's very first growth tick finds
    // no passable neighbor and dies immediately (pick_step returns None).
    for dy in -1..=1i32 {
        for dx in -1..=1i32 {
            if dx == 0 && dy == 0 { continue; }
            w.paint(10 + dx, 10 + dy, 0, Material::Rock as u8);
        }
    }
    let id = w.spawn_colony(10, 10);
    assert_eq!(w.colony_tip_count(id), 1, "colony starts with the one spawned tip");
    w.step(); // one growth tick: the boxed-in tip dies
    assert_eq!(w.tip_count(), 0, "the boxed-in tip should have died");
    assert_eq!(
        w.colony_tip_count(id),
        0,
        "colony.tip_count should be recomputed from live tips, not left stale at spawn's 1"
    );
}

#[test]
fn eating_soil_fills_the_colony_pool() {
    let mut w = World::new(64, 64);
    for x in 30..50 { for y in 30..40 { w.paint(x as i32, y as i32, 0, Material::Soil as u8); w.set_soil_richness(x, y, 150); } }
    let id = w.spawn_colony(32, 35);
    let before = w.colony_pool(id);
    // Mycelium growth only runs 1 frame in P_MY_GROWTH_INTERVAL (3), so triple the step budget
    // (300) to still afford ~100 growth ticks.
    for _ in 0..300 { w.step(); }
    assert!(w.colony_pool(id) > before, "eating rich soil should fill the pool");
}

#[test]
fn well_fed_colony_branches_up_to_the_cap() {
    // World dims must be multiples of CHUNK (64); the brief's 96x96 example doesn't satisfy
    // that (a pre-existing World::new invariant, unrelated to this task), so this uses 128x128
    // with a proportionally scaled rich-soil field around the colony.
    let mut w = World::new(128, 128);
    for x in 10..118 { for y in 40..90 { w.paint(x as i32, y as i32, 0, Material::Soil as u8); w.set_soil_richness(x, y, 220); } }
    w.spawn_colony(64, 65);
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
    // Growth (up to STARVE_GRACE_TICKS=90 growth ticks) lays down a strand of up to ~90 cells,
    // plus whatever P_MY_BRANCH_CHANCE branches add; recede then has to walk all of that back at
    // P_MY_DIEBACK (default 1) cell per growth tick, so this needs a much bigger budget than the
    // ~90-growth-tick grace period alone -- 2000 world steps is ~666 growth ticks, comfortably
    // enough to grow and then fully recede before the assertions below.
    for _ in 0..2000 { w.step(); }
    w.step();
    assert_eq!(w.tip_count(), 0, "starved tips die");
    assert_eq!(w.cells_processed, 0, "receded, settled world sleeps");
}

fn count_mycelium(w: &World, dims: (usize, usize)) -> usize {
    let (width, height) = dims;
    let mut n = 0;
    for x in 0..width {
        for y in 0..height {
            if w.get(x, y) == Material::Mycelium { n += 1; }
        }
    }
    n
}

fn count_material(w: &World, dims: (usize, usize), mat: Material) -> usize {
    let (width, height) = dims;
    let mut n = 0;
    for x in 0..width {
        for y in 0..height {
            if w.get(x, y) == mat { n += 1; }
        }
    }
    n
}

#[test]
fn fed_colony_fruits_and_spends_pool() {
    let mut w = World::new(64, 64);
    // Colony sits in open space with plenty of empty headroom above -- has_fruiting_room and
    // try_fruit's footprint fit-check should succeed against a nearly-empty world.
    let id = w.spawn_colony(32, 40);
    w.set_colony_pool(id, 500); // above P_MY_FRUIT_THRESHOLD (400)
    let before_pool = w.colony_pool(id);
    let before_len = w.mushroom_len();
    // my_grow_countdown starts at 0, so the next step is a growth tick; give a small budget of
    // growth ticks in case the tip's first extend moves it somewhere fruiting can't fit.
    for _ in 0..30 {
        w.step();
        if w.mushroom_len() > before_len { break; }
    }
    assert!(w.mushroom_len() > before_len, "a fed colony should fruit a mushroom");
    assert!(
        w.colony_pool(id) < before_pool,
        "fruiting should spend the pool (before={before_pool}, after={})",
        w.colony_pool(id)
    );
}

#[test]
fn hungry_colony_does_not_fruit() {
    let mut w = World::new(64, 64);
    let id = w.spawn_colony(32, 40);
    w.set_colony_pool(id, 100); // below P_MY_FRUIT_THRESHOLD (400)
    let before_len = w.mushroom_len();
    for _ in 0..30 {
        w.step();
    }
    assert_eq!(w.mushroom_len(), before_len, "a pool below threshold should never fruit");
}

#[test]
fn mushroom_decays_after_lifespan() {
    let mut w = World::new(64, 64);
    // Small lifespan so the test doesn't need thousands of steps.
    w.params.values[sandgun_core::params::P_MUSH_LIFESPAN] = 5.0;
    // Grow a mushroom directly (no colony needed) at a spot with plenty of open headroom.
    assert!(w.try_fruit(32, 40), "mushroom should fit in open space");
    // grow_mushrooms is driven from grow_mycelium on the P_MY_GROWTH_INTERVAL cadence; step
    // until the mushroom finishes revealing and moves off the growing list.
    for _ in 0..500 {
        w.step();
        if w.mushroom_len() == 0 { break; }
    }
    assert_eq!(w.mushroom_len(), 0, "mushroom should finish revealing");
    let flesh_before = count_material(&w, (64, 64), Material::MushroomFlesh);
    assert!(flesh_before > 0, "a completed mushroom should have flesh cells");
    // Run well past the (small) lifespan so it crumbles, plus enough steps for the world to
    // settle back to sleep afterward.
    for _ in 0..500 {
        w.step();
    }
    w.step();
    let flesh_after = count_material(&w, (64, 64), Material::MushroomFlesh);
    assert_eq!(flesh_after, 0, "mushroom flesh should crumble away after its lifespan expires");
    assert_eq!(w.cells_processed, 0, "world should settle again once decay finishes");
}

#[test]
fn mushroom_reveals_at_one_rate_not_double() {
    // Regression (M1e task 4 review): both the old dormant grow() and the new grow_mycelium()
    // called grow_mushrooms() on the same shared self.mushrooms/self.caps. Both cadences default
    // to P_GROWTH_INTERVAL/P_MY_GROWTH_INTERVAL == 3 with countdowns starting at 0, so a single
    // growth tick revealed 2x P_MUSH_REVEAL cells instead of 1x. grow_mycelium (mycelium.rs) is
    // now the sole owner of mushroom reveal/decay.
    let mut w = World::new(64, 64);
    assert!(w.try_fruit(32, 40), "mushroom should fit in open space");
    let reveal = w.params.values[sandgun_core::params::P_MUSH_REVEAL] as usize;
    let before = count_material(&w, (64, 64), Material::MushroomFlesh);
    assert_eq!(before, 0, "freshly fruited mushroom shouldn't have revealed any flesh yet");
    // Step exactly one growth-interval's worth of frames. grow_countdown/my_grow_countdown both
    // start at 0, so the growth tick(s) fire on the very first step and the cadence stays quiet
    // for the rest of the interval -- this window contains exactly one tick from each cadence.
    let interval = w.params.values[sandgun_core::params::P_GROWTH_INTERVAL] as usize;
    for _ in 0..interval {
        w.step();
    }
    let after = count_material(&w, (64, 64), Material::MushroomFlesh);
    let revealed = after - before;
    assert_eq!(
        revealed, reveal,
        "mushroom reveal must run once per growth tick (P_MUSH_REVEAL={reveal}), not twice (got {revealed})"
    );
}

#[test]
fn dieback_reverts_multiple_cells_per_tick() {
    let mut w = World::new(64, 64);
    // No branching, so there's exactly one tip growing one strand -- keeps the cell count
    // delta attributable purely to recede_tip, not try_branch.
    w.params.values[sandgun_core::params::P_MY_BRANCH_CHANCE] = 0.0;
    w.params.values[sandgun_core::params::P_MY_DIEBACK] = 3.0;
    // colony in EMPTY space (no soil to eat) -> pool stays 0 -> starves after the grace period,
    // but until then the tip freely wanders/extends into empty cells, laying a multi-cell strand.
    w.spawn_colony(32, 32);
    // Grace period is 90 growth ticks (age_ticks > 90 is starving); my_grow_countdown starts at
    // 0 so a world step is a growth tick every P_MY_GROWTH_INTERVAL (3) steps. Run exactly the
    // grace period's worth of growth ticks so the colony is one tick shy of starving.
    for _ in 0..(90 * 3) { w.step(); }
    let before = count_mycelium(&w, (64, 64));
    assert!(
        before >= 4,
        "need a multi-cell strand to exercise multi-cell dieback (only {before} mycelium cells)"
    );
    // One more growth tick: now starving (age_ticks == 91 > 90) -> recede_tip should revert
    // P_MY_DIEBACK (3) cells this tick, not just 1.
    for _ in 0..3 { w.step(); }
    let after = count_mycelium(&w, (64, 64));
    let reverted = before - after;
    assert!(
        reverted >= 3,
        "P_MY_DIEBACK=3 should revert ~3 cells in a single recede tick, only {reverted} reverted"
    );
}

#[test]
fn winding_strand_fully_recedes_no_stubs() {
    let mut w = World::new(64, 64);
    // A single wandering tip in open space naturally winds (momentum bias competes with the
    // per-step RNG jitter in pick_step), so this exercises recede_tip's "follow the actual
    // strand backward" behavior rather than a straight-line special case.
    w.params.values[sandgun_core::params::P_MY_BRANCH_CHANCE] = 0.0;
    w.spawn_colony(32, 32);
    // Grow past the grace period so it starts starving, then keep going long enough to fully
    // unwind whatever strand it grew (grace ~90 growth ticks to grow, plus generous budget to
    // recede at the default dieback rate) and for the world to settle.
    for _ in 0..(400 * 3) { w.step(); }
    w.step();
    assert_eq!(w.tip_count(), 0, "starved tip should have died once fully receded");
    assert_eq!(
        count_mycelium(&w, (64, 64)),
        0,
        "winding strand should fully unwind -- no leftover dead-mycelium stubs"
    );
    assert_eq!(w.cells_processed, 0, "receded, settled world sleeps");
}

// --- Task 5: support/anchor -- carve-flood; unsupported chunks fall & die ---

#[test]
fn severed_mycelium_bridge_falls() {
    let mut w = World::new(64, 64);
    w.paint(5, 32, 0, Material::Rock as u8); // anchor
    // a long mycelium bridge from the anchor (x=6, touching the rock) out to a floating far end
    for x in 6..40 { w.paint(x as i32, 32, 0, Material::Mycelium as u8); }
    let particles_before = w.particle_count();
    // carve straight down into the middle of the bridge (x=23), splitting it in two
    w.fire(23.5, 27.0, 0.0, 8.0, Ammo::Kinetic as u8);
    w.step(); // impact + carve + the resulting support check, all this frame
    assert!(
        w.particle_count() > particles_before,
        "the disconnected far piece should become falling particles"
    );
    for _ in 0..400 { w.step(); } // let debris fall/settle
    // near side (still connected to the rock through x=6) should remain
    assert!(
        (6..15).any(|x| w.get(x, 32) == Material::Mycelium),
        "the side still connected to the anchor should stay put"
    );
    // far side (was disconnected by the carve, no anchor within reach) should have fallen away
    for x in 30..39 {
        assert_ne!(
            w.get(x, 32),
            Material::Mycelium,
            "disconnected piece at x={x} should have fallen, not stayed in place"
        );
    }
}

#[test]
fn anchored_mycelium_stays() {
    let mut w = World::new(64, 64);
    w.paint(5, 32, 0, Material::Rock as u8); // anchor
    // a short strand entirely rooted at the anchor -- carving its far end must not drop the part
    // that's still connected all the way back to the rock.
    for x in 6..21 { w.paint(x as i32, 32, 0, Material::Mycelium as u8); }
    w.fire(17.5, 27.0, 0.0, 8.0, Ammo::Kinetic as u8); // carve near the far end, away from the rock
    w.step();
    for _ in 0..200 { w.step(); }
    assert!(
        (6..11).all(|x| w.get(x, 32) == Material::Mycelium),
        "mycelium still connected to an anchor must not be dropped by a nearby carve"
    );
}

#[test]
fn settle_after_drop() {
    let mut w = World::new(64, 64);
    for x in 0..64 { w.paint(x as i32, 63, 0, Material::Rock as u8); } // floor to catch debris
    w.paint(5, 32, 0, Material::Rock as u8); // anchor
    for x in 6..40 { w.paint(x as i32, 32, 0, Material::Mycelium as u8); }
    w.fire(23.5, 27.0, 0.0, 8.0, Ammo::Kinetic as u8);
    w.step();
    for _ in 0..400 { w.step(); }
    assert_eq!(w.particle_count(), 0, "dropped debris should have landed by now");
    w.step();
    assert_eq!(w.cells_processed, 0, "world should settle again once the drop resolves");
}

#[test]
fn tip_on_removed_cell_dies_instead_of_regrowing_from_nothing() {
    let mut w = World::new(64, 64);
    w.spawn_colony(32, 32); // tip starts exactly on this cell, with no anchor nearby
    assert_eq!(w.tip_count(), 1);
    // carve straight through the colony's own origin/tip cell
    w.fire(27.5, 32.5, 12.0, 0.0, Ammo::Kinetic as u8);
    w.step(); // impact clears (32, 32) this frame
    assert_eq!(w.get(32, 32), Material::Empty, "the carve should have cleared the tip's cell");
    for _ in 0..10 { w.step(); } // let a growth tick run with the tip's cell gone
    assert_eq!(
        w.tip_count(),
        0,
        "a tip whose current cell is no longer Mycelium must die, not regrow from nothing"
    );
}

// --- Task 6: switchover -- worldgen seeds colonies; old M1c growth model removed ---
//
// Ammo::Spore used to paint inert mycelium and separately register it with the old growth
// model so it would actually grow. That model is gone: Spore ammo now plants a living COLONY
// directly (spawn_colony), which grows on its own via the organism model with no extra wiring.

#[test]
fn spore_ammo_plants_a_growing_colony() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 41, 0, Material::Rock as u8); // floor so the soil can't avalanche
        for y in 30..40 {
            w.paint(x, y, 0, Material::Soil as u8);
        }
    }
    for x in 0..64usize {
        for y in 30..40usize {
            w.set_soil_richness(x, y, 150);
        }
    }

    let colonies_before = w.colony_count();
    w.fire(5.0, 35.0, 12.0, 0.0, Ammo::Spore as u8); // fired into the soil band
    w.step();
    assert!(w.colony_count() > colonies_before, "spore ammo should plant a new living colony on impact");

    let before = count_mycelium(&w, (64, 64));
    for _ in 0..300 {
        w.step();
    }
    let after = count_mycelium(&w, (64, 64));
    assert!(after > before, "the planted colony should actually grow ({before} -> {after})");
}

// --- Parametric mushroom shape (kept from the old M1c growth model; fruiting is triggered by
// the colony economy above, but the shape/reveal/decay machinery below is unchanged) ---

fn ash_count(w: &World) -> usize {
    let mut n = 0;
    for y in 0..w.height {
        for x in 0..w.width {
            if w.get(x, y) == Material::Ash {
                n += 1;
            }
        }
    }
    n
}

fn fire_count(w: &World) -> usize {
    let mut n = 0;
    for y in 0..w.height {
        for x in 0..w.width {
            if w.get(x, y) == Material::Fire {
                n += 1;
            }
        }
    }
    n
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

#[test]
fn burning_flesh_puffs_non_burning_spores() {
    // Regression: carve_crater must fully reset a carved MushroomFlesh cell's state when it
    // leaves SporeGas behind. If flags (specifically FLAG_BURNING) survive the conversion, the
    // fresh spore cell has aux=0 (SporeGas's initial aux) but is still flagged burning, so the
    // very next tick's dispatch checks FLAG_BURNING before material, routes to update_burning,
    // sees aux==0 ("fuel spent"), and detonates SporeGas -> Fire with no real fire nearby.
    let mut w = World::new(64, 64);
    w.set_param(sandgun_core::params::P_GUNFIRE_SPORE_CHANCE as u32, 1.0); // always puff on carve
    w.set_param(sandgun_core::params::P_FLAM_FLESH as u32, 1.0); // guarantee ignition on contact

    // A single mushroom flesh cell, ignited via the normal fire-spread path (not by poking
    // flags directly) so this reproduces the real "fire already spread to a colony" scenario.
    w.paint(25, 32, 0, Material::MushroomFlesh as u8);
    w.paint(24, 32, 0, Material::Fire as u8); // adjacent flame catches the flesh next step
    w.step();
    assert_ne!(w.cell_flags(25, 32) & sandgun_core::cell::FLAG_BURNING, 0, "setup: flesh should have caught fire");

    // Remove the igniting fire so nothing but the burning flesh cell remains nearby.
    w.paint(24, 32, 0, Material::Empty as u8);

    // Shoot a Kinetic round into the still-burning flesh cell (travels 20 world units in a
    // single ray-march call, so the impact + carve resolve within this next step()).
    w.fire(5.0, 32.0, 20.0, 0.0, 0); // Kinetic = 0
    w.step();

    // Immediately after the impact resolves (before any spore could legitimately meet fire),
    // the carved cell must not be a pre-burning spore.
    assert_eq!(w.get(25, 32), Material::SporeGas, "carving a burning flesh cell should leave spore gas");
    assert_eq!(
        w.cell_flags(25, 32) & sandgun_core::cell::FLAG_BURNING,
        0,
        "a freshly carved spore cell must not inherit FLAG_BURNING from the flesh it replaced"
    );

    // And it must not phantom-detonate on the following tick with no real fire nearby (checked
    // world-wide since a rising spore gas cell may have drifted off its carve coordinates).
    w.step();
    assert_eq!(fire_count(&w), 0, "a carved spore cell must not self-detonate absent real fire");
}

#[test]
fn burning_mushroom_leaves_sparse_ash() {
    // Regression: burnt-out Mycelium/MushroomFlesh always converted 1:1 to Ash. That's far too
    // much ash for a burned mushroom colony -- it should mostly disappear (Empty), leaving only
    // some ash behind.
    let mut w = World::new(64, 64);
    w.set_param(sandgun_core::params::P_FLAM_FLESH as u32, 1.0); // guarantee ignition on contact
    w.set_param(sandgun_core::params::P_FUEL_FLESH as u32, 1.0); // burn out almost immediately

    // M1e task 5: unsupported Mycelium/MushroomFlesh detaches and falls when a carve/burn
    // disturbs it. A flesh slab with no anchor at all would be judged unsupported the moment the
    // first edge cell burns away, dropping the whole remaining slab as unburnt falling debris
    // before it can finish burning to ash -- which is the right call for a truly floating mass,
    // but not what this test (burn-to-ash ratio) is exercising. Anchor it to rock along the
    // bottom, like a mushroom actually growing on the ground, so it stays put and burns through.
    for x in 19..31 {
        w.paint(x, 30, 0, Material::Rock as u8);
    }

    let mut flesh_count = 0usize;
    for x in 20..30 {
        for y in 20..30 {
            w.paint(x, y, 0, Material::MushroomFlesh as u8);
            flesh_count += 1;
        }
    }
    w.paint(19, 25, 0, Material::Fire as u8); // ignite the slab from one edge

    for _ in 0..200 {
        w.step();
    }

    let ash = ash_count(&w);
    assert!(ash > 0, "some ash should form");
    assert!(
        (ash as f32) < 0.6 * flesh_count as f32,
        "ash should be sparse, not 1:1 with burnt flesh ({ash} ash / {flesh_count} flesh)"
    );
}

#[test]
fn mushroom_does_not_overwrite_soil() {
    // Regression: reveal_mushroom used to write MushroomFlesh into Soil as well as Empty,
    // letting a growing mushroom carve straight through solid ground. It must only ever
    // occupy Empty cells. The soil column is trapped in a rock channel (rock on both sides,
    // rock floor below) so it physically cannot avalanche away under gravity -- isolating the
    // pure "must not overwrite soil" behavior from unrelated powder-physics sliding.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8); // floor so the mushroom has room to grow up
    }
    for y in 20..50 {
        w.paint(34, y, 0, Material::Rock as u8); // channel wall
        w.paint(35, y, 0, Material::Soil as u8); // soil, trapped in the channel
        w.paint(36, y, 0, Material::Rock as u8); // channel wall
    }
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, 10.0);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, 10.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, 5.0);
    w.try_fruit(32, 49); // dome would normally span roughly x in [25,39], overlapping x=35

    for _ in 0..2000 {
        w.step();
        if w.mushroom_len() == 0 {
            break;
        }
    }
    assert_eq!(w.mushroom_len(), 0, "mushroom must finish growing");
    for y in 20..50 {
        assert_eq!(
            w.get(35, y),
            Material::Soil,
            "mushroom growth must not overwrite soil at (35, {y})"
        );
    }
}

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
        // The stem wanders gently (+/-2 cells max, see try_fruit's sway_seed), so check a
        // small window around the base x rather than the exact column.
        if (30..=34).any(|x| w.get(x, 45) == Material::MushroomFlesh) {
            saw_stem = true;
        }
        if w.mushroom_len() == 0 {
            break;
        }
    }
    assert!(saw_stem, "stem should have been revealed above the base");
    assert_eq!(w.mushroom_len(), 0, "completed mushroom must retire from the list");
    // A cap cell exists to the SIDE of the stem -- excluding a window around the stem's own
    // (gently wandering, +/-2 cell) column so this genuinely checks for the cap dome, not just
    // more stem. With height in [6,16] and cap_r in [3,7] (default params) plus the +/-2 sway,
    // the dome (which sits only at/above the stem top) always falls within y in [26, 44] and
    // x in [23, 41].
    let cap_present = (23..42).any(|x| {
        !(30..=34).contains(&x) && (26..45).any(|y| w.get(x, y) == Material::MushroomFlesh)
    });
    assert!(cap_present, "cap dome should have been revealed to the side of the stem");
}

#[test]
fn mushroom_cap_is_a_dome_not_a_ball() {
    // Deterministic shape: height=10, cap_r=5, base_y=49 -> stem spans y in [39, 48] around
    // x=32 (with a gentle +/-2 cell wander), and the dome (upper hemisphere) must sit AT/ABOVE
    // the stem top (y in [34, 39]), never bulging out to the sides below the stem top. A
    // full-circle cap bug would paint flesh beside the stalk in rows y in [40, 44]
    // (cap_top_y+1 .. cap_top_y+r) -- that is exactly what this test forbids.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8); // floor so the mushroom has room to grow up
    }
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, 10.0);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, 10.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, 5.0);
    w.try_fruit(32, 49); // base_y=49, height=10 -> stem top (cap_top_y) = 39

    for _ in 0..2000 {
        w.step();
        if w.mushroom_len() == 0 {
            break;
        }
    }
    assert_eq!(w.mushroom_len(), 0, "mushroom must finish growing");

    // The stem itself is present near x=32 partway up (sanity check the shape actually grew;
    // the stem may wander +/-2 cells from the base x, hence the small window).
    assert!(
        (30..=34).any(|x| w.get(x, 45) == Material::MushroomFlesh),
        "stem should be present near x=32"
    );

    // Locate the dome's ACTUAL center via its widest row (cap_top_y = 39): it must be one
    // contiguous run of width 2*cap_r+1 = 11, wherever the (possibly wandered) stem top put it.
    let cap_top_y = 39usize;
    let dome_xs: Vec<i32> =
        (20..=44).filter(|&x| w.get(x as usize, cap_top_y) == Material::MushroomFlesh).collect();
    assert_eq!(dome_xs.len(), 11, "dome's widest row at the stem top should be fully filled (width 11)");
    let (dmin, dmax) = (*dome_xs.iter().min().unwrap(), *dome_xs.iter().max().unwrap());
    assert_eq!(dmax - dmin + 1, 11, "dome's widest row must be one contiguous run, not scattered");
    let dome_center = (dmin + dmax) / 2;

    // No flesh beside the stalk below the stem top (y in [40, 44] = cap_top_y+1 ..= cap_top_y+r),
    // except within a small window around the stem's own (gently wandering) column: a
    // full-circle cap would bulge flesh out to dome_center +/- r here; a dome must not. The
    // wander amplitude is capped at 2, and cap_r=5 here, so the excluded window (5 cells wide)
    // can never swallow the whole dome width (11 cells).
    for y in 40..=44 {
        for x in (dome_center - 5)..=(dome_center + 5) {
            if (30..=34).contains(&x) {
                continue; // the stem's own (slightly wandering) column
            }
            assert_eq!(
                w.get(x as usize, y),
                Material::Empty,
                "cap must not bulge below the stem top at ({x}, {y})"
            );
        }
    }

    // The dome curves upward above the stem top toward its apex near y = 34.
    let dome_reaches_apex = (34..=38)
        .any(|y| (dome_center - 5..=dome_center + 5).any(|x| w.get(x as usize, y) == Material::MushroomFlesh));
    assert!(dome_reaches_apex, "dome should curve upward above the stem top");
}

#[test]
fn mushroom_cap_reveals_bottom_row_before_apex() {
    // Regression: cap_dome() used to be ordered top-down (dy from -r to 0), so the very FIRST
    // cap cell revealed was the apex -- a single flesh cell floating above the stem before the
    // rest of the dome filled in. It must reveal bottom-up so the cap stays grounded/connected
    // to the stem top throughout its growth.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8);
    }
    let height: i32 = 5;
    let cap_r: i32 = 5;
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, height as f32);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, height as f32);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, cap_r as f32);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, cap_r as f32);
    w.set_param(sandgun_core::params::P_MUSH_REVEAL as u32, 1.0); // exactly one cell revealed per growth tick
    // Mushroom reveal is driven solely by grow_mycelium's cadence, so it's P_MY_GROWTH_INTERVAL
    // that must be 1 here.
    w.set_param(sandgun_core::params::P_MY_GROWTH_INTERVAL as u32, 1.0); // one growth tick per step

    let base_x = 32i32;
    let base_y = 59i32;
    w.try_fruit(base_x as usize, base_y as usize);

    // `height` steps fully reveal the stem (progress == stem == height); the (height+1)th
    // step reveals exactly the FIRST entry of cap_dome(cap_r) -- nothing else.
    for _ in 0..(height + 1) {
        w.step();
    }

    let cap_top_y = (base_y - height) as usize;
    let apex_y = (cap_top_y as i32 - cap_r) as usize;
    let window = cap_r + 2; // covers the stem's +/-2 sway plus the dome radius

    let bottom_row_filled = (base_x - window..=base_x + window)
        .any(|x| w.get(x as usize, cap_top_y) == Material::MushroomFlesh);
    let apex_filled = (base_x - window..=base_x + window)
        .any(|x| w.get(x as usize, apex_y) == Material::MushroomFlesh);

    assert!(
        bottom_row_filled,
        "the first cap cell revealed should be on the widest row, flush against the stem top"
    );
    assert!(!apex_filled, "the apex must not be revealed before the widest row grounds the cap");
}

#[test]
fn mushrooms_do_not_overlap() {
    // Regression: reveal_mushroom only ever writes into Empty cells (won't overwrite), but
    // nothing checked at FRUITING time that the whole footprint was clear, so adjacent
    // mushrooms could be enqueued back-to-back and interleave/merge as they both grew into the
    // same space. A fit-check at fruiting time must reject a mushroom whose footprint would
    // land on top of an already-grown one.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8); // floor, open headroom above
    }
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, 5.0);

    assert!(w.try_fruit(32, 59), "first mushroom should spawn freely in open space");
    // Let the first mushroom fully grow before the second tries to fruit into overlapping
    // ground, so the fit-check has real flesh cells (not just a pending plan) to detect.
    for _ in 0..2000 {
        w.step();
        if w.mushroom_len() == 0 {
            break;
        }
    }
    assert_eq!(w.mushroom_len(), 0, "setup: first mushroom should finish growing");

    // Second mushroom's base is only 2 cells away -- with cap_r=5 (diameter 11) its footprint
    // necessarily overlaps the first mushroom's now-solid flesh, regardless of stem sway.
    let spawned = w.try_fruit(34, 59);
    assert!(!spawned, "a mushroom whose footprint overlaps another must not spawn");
    assert_eq!(w.mushroom_len(), 0, "an overlapping mushroom must not be added to the growing list");
}

#[test]
fn mushroom_needs_clear_footprint() {
    // Regression: fruiting had no whole-footprint check, so a mycelium cell whose cap would
    // land on an obstruction (ceiling, another mushroom, etc.) could still fruit and grow a
    // stem straight into it, letting the cap dome carve through or interleave with solid cells.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8); // floor
    }
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, 5.0);
    // With base_y=59 and height=5, the stem occupies rows y in [54, 58]; the cap dome (radius 5)
    // occupies rows y in [49, 54]. Block a wide rock ceiling across rows [49, 53] -- pure dome
    // territory, wide enough to cover the stem's +/-2 sway -- while leaving the stem's own rows
    // open, isolating "cap footprint obstructed" from "stem footprint obstructed".
    for x in 20..45 {
        for y in 49..=53 {
            w.paint(x, y, 0, Material::Rock as u8);
        }
    }

    let spawned = w.try_fruit(32, 59);
    assert!(!spawned, "a mushroom whose cap footprint is obstructed must not spawn");
    assert_eq!(w.mushroom_len(), 0, "an obstructed mushroom must not be added to the growing list");
}

#[test]
fn mushroom_stems_vary_and_stay_connected() {
    // Regression: mushroom stems used to be a perfectly straight column at the mushroom's base
    // x. They should now wander gently (deterministically, seeded per-mushroom) while staying
    // visually connected, and the cap dome must follow the stem's ACTUAL top, not the base x.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8); // floor, wide open headroom above
    }
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, 12.0);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, 12.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, 4.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, 4.0);

    // Two mushrooms far enough apart to never interact; try_fruit draws sway_seed from the sim
    // RNG each call, so the two mushrooms get different sway even with identical height/cap.
    w.try_fruit(15, 59);
    w.try_fruit(45, 59);

    for _ in 0..3000 {
        w.step();
        if w.mushroom_len() == 0 {
            break;
        }
    }
    assert_eq!(w.mushroom_len(), 0, "both mushrooms must finish growing");

    let mut all_stems = Vec::new();
    for base_x in [15i32, 45i32] {
        // Pure-stem rows: p = 0..=10 (height=12, so p=11 is the shared stem-top/dome row,
        // checked separately below).
        let mut stem_xs = Vec::new();
        for p in 0..=10i32 {
            let y = 59 - 1 - p;
            let found = (base_x - 4..=base_x + 4)
                .find(|&x| w.get(x as usize, y as usize) == Material::MushroomFlesh);
            stem_xs.push(found.expect("stem cell missing at expected height"));
        }
        // (b) connected: consecutive stem cells differ by at most 1 in x -- no gaps.
        for pair in stem_xs.windows(2) {
            assert!((pair[0] - pair[1]).abs() <= 1, "stem must stay connected: {stem_xs:?}");
        }
        all_stems.push((base_x, stem_xs));
    }

    // (a) at least one stem is NOT perfectly straight (some stem cell x != base x).
    let any_wavy = all_stems.iter().any(|(base_x, xs)| xs.iter().any(|&x| x != *base_x));
    assert!(any_wavy, "at least one stem should wander away from its base x");

    // (c) the cap dome sits atop the stem's ACTUAL top, not the mushroom's base x.
    for (base_x, xs) in &all_stems {
        let cap_top_y = (59 - 12) as usize; // base_y - height = 47
        let dome_xs: Vec<i32> = (base_x - 8..=base_x + 8)
            .filter(|&x| w.get(x as usize, cap_top_y) == Material::MushroomFlesh)
            .collect();
        assert!(!dome_xs.is_empty(), "dome's widest row should be present at the stem top");
        let (dmin, dmax) = (*dome_xs.iter().min().unwrap(), *dome_xs.iter().max().unwrap());
        assert_eq!(dmax - dmin + 1, dome_xs.len() as i32, "dome widest row should be one contiguous run");
        let dome_center = (dmin + dmax) / 2;
        let last_stem_x = *xs.last().unwrap();
        assert!(
            (dome_center - last_stem_x).abs() <= 1,
            "cap dome (centered at {dome_center}) must sit atop the stem's actual top \
             ({last_stem_x}), not just the base x ({base_x})"
        );
    }
}
