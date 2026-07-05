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
