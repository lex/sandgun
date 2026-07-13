use sandgun_core::cell::Material;
use sandgun_core::projectile::Ammo;
use sandgun_core::world::World;
use sandgun_core::worldgen;

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
    // M1e playtest fixes 3/4 (branching + wandering, and 2-wide strands) mean growth needs real
    // 2D room to spread into: a single-cell-wide soil corridor is a pathological bottleneck where
    // a branch's own perpendicular thickening (fix 4) or diagonal wander (fix 3) can consume the
    // one soil cell directly ahead of the main tip before it ever gets there. So this uses a soil
    // BAND several rows tall (not a single row) with richness increasing left-to-right -- poor
    // near the colony's spawn end, richest at the far end -- so branches have slack to explore
    // without starving the main growth front of its next step. Pinned to a rock floor -- soil is
    // granular and would otherwise avalanche away before the tip ever senses it.
    for x in 30..52 { w.paint(x, 40, 0, Material::Rock as u8); }
    for x in 30..50 {
        for y in 32..40 {
            w.paint(x as i32, y as i32, 0, Material::Soil as u8);
            let richness = ((x - 30) * 10).min(200) as u8;
            w.set_soil_richness(x, y, richness);
        }
    }
    w.spawn_colony(30, 35); // poor (spawn) end of the band
    for _ in 0..3000 { w.step(); }
    // mycelium should have advanced rightward into the rich soil
    let reached = (44..50).any(|x| (32..40).any(|y| w.get(x, y) == Material::Mycelium));
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
    // Sample the pool as soon as it grows, rather than requiring it to hold at the end of a long
    // fixed run: this soil field has no rock floor, so it can (correctly) avalanche and eventually
    // starve/reap the colony -- with the M1e task 1 reap pass, a colony with no live tips has its
    // pool zeroed and is reaped once its cells are gone, which would read back as pool==0 and
    // wrongly look like it never ate, even though it demonstrably did earlier in the same run.
    let mut grew = false;
    for _ in 0..300 {
        w.step();
        if w.colony_pool(id) > before {
            grew = true;
            break;
        }
    }
    assert!(grew, "eating rich soil should fill the pool");
}

#[test]
fn well_fed_colony_branches_up_to_the_cap() {
    // World dims must be multiples of CHUNK (64); the brief's 96x96 example doesn't satisfy
    // that (a pre-existing World::new invariant, unrelated to this task), so this uses 128x128
    // with a proportionally scaled rich-soil field around the colony.
    let mut w = World::new(128, 128);
    // Rock floor under the soil field: without it, this whole unsupported block of granular Soil
    // continuously avalanches (nothing below y=90 was ever solid), constantly opening and closing
    // Empty pockets throughout the mass. The old growth model tolerated that by growing straight
    // through however much open air appeared; M1e playtest fix 2's air-reach cap does not, so an
    // ever-shifting sandpile starves every tip of substrate and the whole colony dies out. A solid
    // floor keeps the packed block stable (no local voids to slump into) like real ground.
    for x in 9..119 { w.paint(x, 90, 0, Material::Rock as u8); }
    for x in 10..118 { for y in 40..90 { w.paint(x as i32, y as i32, 0, Material::Soil as u8); w.set_soil_richness(x, y, 220); } }
    w.spawn_colony(64, 65);
    let cap = 12; // P_MY_TIP_CAP default
    // M1e playtest fix 1 slowed the growth cadence (P_MY_GROWTH_INTERVAL 3 -> 8, ~2.7x fewer
    // growth ticks per step), so scale the step budget up to match. Track the PEAK tip count
    // across the run (like global_mushroom_cap_is_respected does for mushrooms) rather than
    // just checking the final tick: up to 12 tips concurrently tunneling through the same soil
    // mass under fix 4's 2-wide strands can hollow it out enough that some regions cave in
    // (soil above a tunnel slumping into the void), so a heavily-fed colony can branch all the
    // way to the cap and *later* start starving/receding as its local substrate runs out --
    // that's a real, correct part of the growth/recede lifecycle, not a failure to branch.
    let mut max_tips = 0usize;
    for _ in 0..900 {
        w.step();
        max_tips = max_tips.max(w.tip_count());
        assert!(w.tip_count() <= cap, "tips must not exceed the cap");
    }
    assert!(max_tips > 1, "a fed colony should branch");
}

#[test]
fn strand_is_at_least_two_wide_and_orthogonally_connected() {
    // M1e playtest fix 4: strands should be P_MY_STRAND_WIDTH (default 2) cells wide. A diagonal
    // step's two endpoints are otherwise only corner-connected to each other (8-connectivity, not
    // 4-connectivity) -- exactly the straight-45-degree, corner-only-connected problem fix 3
    // calls out -- UNLESS something also fills in one of the two orthogonal "elbow" cells between
    // them, which is exactly what thicken_strand's extra cell does for a diagonal move.
    //
    // This is deliberately a single, deterministic diagonal step rather than a long organic
    // random walk: every neighbor of the spawn point is walled off with Rock except one diagonal
    // target cell (rich Soil, so pick_step picks it regardless of RNG -- richness+soil_bonus
    // dwarfs the wander term) and the two orthogonal elbow cells that would bridge spawn->target,
    // left Empty so thickening has somewhere to go. Over hundreds of organic growth steps a
    // wiggly path can coincidentally end up passing near itself and fill an "elbow" cell purely
    // by chance (unrelated to thickening), which would make a scan-the-whole-strand version of
    // this test pass even with thickening disabled -- pinning down one exact step rules that out.
    let mut w = World::new(64, 64);
    // Block every neighbor of (32,32) except the diagonal target (33,33) and the two elbow cells
    // (33,32), (32,33) (left as default Empty).
    for (dx, dy) in [(-1i32, -1i32), (0, -1), (1, -1), (-1, 0), (-1, 1)] {
        w.paint(32 + dx, 32 + dy, 0, Material::Rock as u8);
    }
    // Soil is granular: without support directly (and diagonally) below, the lone Soil cell at
    // (33,33) would fall/slide away under gravity (which runs before growth within the same
    // step()) before the tip ever gets a chance to sense it.
    for x in 32..=34 { w.paint(x, 34, 0, Material::Rock as u8); }
    w.paint(33, 33, 0, Material::Soil as u8);
    w.set_soil_richness(33, 33, 150);
    let id = w.spawn_colony(32, 32);
    w.step(); // my_grow_countdown starts at 0, so this is exactly one growth tick
    assert_eq!(
        w.get(33, 33),
        Material::Mycelium,
        "setup: the tip's only real candidate is the rich diagonal soil cell"
    );
    let elbow_filled = (w.get(33, 32) == Material::Mycelium && w.cell_aux(33, 32) == id)
        || (w.get(32, 33) == Material::Mycelium && w.cell_aux(32, 33) == id);
    assert!(
        elbow_filled,
        "a diagonal step should also fill an orthogonal elbow cell (strand width >= 2) -- \
         otherwise the pair is left only corner-connected"
    );
}

#[test]
fn mycelium_does_not_grow_far_into_open_air() {
    // M1e playtest fix 2: a tip may cross at most P_MY_MAX_AIR_REACH consecutive Empty cells
    // before it must reach Soil again, or it dies rather than sailing further into open sky.
    // Spawn a colony with only a small soil patch nearby and mostly open air everywhere else,
    // then run for a long time: growth must NOT sprawl across the open region indefinitely (the
    // old model, with no air-reach limit and momentum locking onto a long ray, could shoot a
    // strand clear across open space) -- it should stay near the patch, exhaust it, and the
    // world should settle (no live tips, no cells still processing).
    let mut w = World::new(128, 128);
    // A small anchored soil patch, otherwise the entire 128x128 world is open Empty air.
    for x in 60..64 { w.paint(x, 68, 0, Material::Rock as u8); } // floor under the patch
    for x in 60..64 {
        for y in 64..68 {
            w.paint(x as i32, y as i32, 0, Material::Soil as u8);
            w.set_soil_richness(x, y, 80);
        }
    }
    w.spawn_colony(62, 65);
    for _ in 0..6000 {
        w.step();
    }
    w.step();
    assert_eq!(w.tip_count(), 0, "tips should have died out once the small patch was exhausted");
    assert_eq!(
        w.cells_processed, 0,
        "growth must not sprawl across the open region forever -- the world should settle"
    );

    // No mycelium cell should have ended up far from any Soil (or the patch's original
    // footprint): with P_MY_MAX_AIR_REACH default 3, a strand can only reach a handful of cells
    // beyond substrate before it's forced to turn back or die, nowhere near sprawling across the
    // open 128x128 field.
    let far_reach = 20usize; // generous margin well beyond a few air-reach hops
    for x in 0..128usize {
        for y in 0..128usize {
            if w.get(x, y) != Material::Mycelium {
                continue;
            }
            let dx = (x as isize - 62).unsigned_abs();
            let dy = (y as isize - 66).unsigned_abs();
            assert!(
                dx < far_reach && dy < far_reach,
                "mycelium cell at ({x},{y}) is implausibly far from the only soil patch (62..66,64..68)"
            );
        }
    }
}

#[test]
fn starving_colony_recedes_and_world_sleeps() {
    let mut w = World::new(64, 64);
    // colony in EMPTY space (no soil to eat) -> pool stays 0 -> starves. Since M1e playtest fix 2
    // (P_MY_MAX_AIR_REACH), a tip with no soil anywhere nearby actually dies from the air-reach
    // cap within just a few growth ticks (pick_step refuses to extend further into open Empty),
    // well before it could ever grow ~90 cells and reach the old starvation-recede path -- so in
    // THIS pure-open-air setup the tip dies boxed-in almost immediately rather than growing then
    // receding. Either way the assertions below hold (no live tips, world settled), so this test
    // still passes; the budget below is generous legacy headroom from when this test relied on a
    // full grow-then-recede cycle.
    w.spawn_colony(32, 32);
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
    // try_fruit's footprint fit-check should succeed against a nearly-empty world. A small soil
    // patch just below the spawn (on a rock floor so it can't avalanche away) keeps the tip
    // within the M1e playtest fix 2 air-reach budget (P_MY_MAX_AIR_REACH) as it wanders, instead
    // of dying in open air before it ever gets a chance to fruit; the headroom above y=40 stays
    // clear either way.
    for x in 10..54 { w.paint(x, 46, 0, Material::Rock as u8); }
    for x in 10..54 { for y in 41..46 { w.paint(x, y, 0, Material::Soil as u8); } }
    let id = w.spawn_colony(32, 40);
    w.set_colony_pool(id, 500); // above P_MY_FRUIT_THRESHOLD (400)
    let before_pool = w.colony_pool(id);
    let before_len = w.mushroom_len();
    // my_grow_countdown starts at 0, so the next step is a growth tick. M1e playtest fix 1 slowed
    // the growth cadence (P_MY_GROWTH_INTERVAL 3 -> 8), so give a bigger budget of growth ticks in
    // case the tip's first several extends move it somewhere fruiting can't fit.
    for _ in 0..300 {
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
    // used to call grow_mushrooms() on the same shared self.mushrooms/self.caps, each on its own
    // cadence, so a single growth tick could reveal 2x P_MUSH_REVEAL cells instead of 1x. That
    // race is now structurally impossible: the old dormant grow() call site (and its
    // grow_countdown/P_GROWTH_INTERVAL cadence) is gone entirely -- grow_mycelium (mycelium.rs)
    // is the sole owner of mushroom reveal/decay, driven only by P_MY_GROWTH_INTERVAL /
    // my_grow_countdown. This test now just pins down that one-tick-one-reveal invariant.
    let mut w = World::new(64, 64);
    assert!(w.try_fruit(32, 40), "mushroom should fit in open space");
    let reveal = w.params.values[sandgun_core::params::P_MUSH_REVEAL] as usize;
    let before = count_material(&w, (64, 64), Material::MushroomFlesh);
    assert_eq!(before, 0, "freshly fruited mushroom shouldn't have revealed any flesh yet");
    // Step exactly one growth-interval's worth of frames. my_grow_countdown starts at 0, so the
    // growth tick fires on the very first step and the cadence stays quiet for the rest of the
    // interval -- this window contains exactly one growth tick.
    let interval = w.params.values[sandgun_core::params::P_MY_GROWTH_INTERVAL] as usize;
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

// --- Review fix: global mushroom cap (repurposes P_MAX_MUSHROOMS) ---

#[test]
fn global_mushroom_cap_is_respected() {
    // Regression (M1e task 6 review): fruit_fed_colonies throttles to one mushroom per alive
    // colony per growth tick, but had no cap on the TOTAL number of simultaneous mushrooms --
    // enough well-fed colonies could keep fruiting forever. P_MAX_MUSHROOMS sat declared but
    // dead (a leftover from the old model); this test drives several fed colonies past a small
    // cap and asserts the in-flight mushroom count never exceeds it.
    let mut w = World::new(128, 128);
    for x in 0..128 {
        w.paint(x, 60, 0, Material::Rock as u8); // floor so every colony has fruiting headroom above
    }
    w.set_param(sandgun_core::params::P_MAX_MUSHROOMS as u32, 3.0);

    // Several colonies, spread out so their fruiting footprints don't collide with each other,
    // each pre-fed above P_MY_FRUIT_THRESHOLD so every one of them wants to fruit every tick its
    // pool allows.
    let mut ids = Vec::new();
    for x in [10usize, 30, 50, 70, 90, 110] {
        let id = w.spawn_colony(x, 55);
        w.set_colony_pool(id, 100_000); // never runs dry across the whole run
        ids.push(id);
    }

    let mut max_seen = 0usize;
    for _ in 0..2000 {
        w.step();
        // "simultaneous" mushrooms means every one currently visible on the map: still growing
        // (mushrooms) or fully grown and counting down to crumble (decaying_mushrooms) both still
        // occupy MushroomFlesh cells on screen.
        let in_flight = w.mushroom_len() + w.decaying_mushroom_len();
        max_seen = max_seen.max(in_flight);
        assert!(
            in_flight <= 3,
            "in-flight mushroom count {in_flight} exceeded cap 3 (P_MAX_MUSHROOMS)"
        );
    }
    assert!(max_seen > 0, "setup: expected at least one mushroom to have fruited during the run");
}

/// Carve a 1-cell-wide, ZERO-richness Soil corridor that zigzags (right/down/right/up/...),
/// walled on every other side by Rock, and spawn a colony at its start. Returns the colony id.
///
/// M1e playtest fixes 3/4 made pick_step considerably more wiggly (weaker momentum, an
/// orthogonal bias, a wider RNG wander term). A LONE tip freely wandering open 2D ground now
/// self-traps (no passable, unvisited neighbor left -- see pick_step's None case) within
/// anywhere from ~15 to ~70 growth ticks, well short of the 90-tick starvation grace period
/// (STARVE_GRACE_TICKS) these tests need to survive to ever reach the starvation-recede path --
/// and a tip that dies boxed-in does NOT recede (by design: that's a "stopped growing, keep the
/// structure" event, not "no food anywhere, shrink back", see `tip_count_tracks_live_tips`),
/// so a self-trapped tip would leave a permanent, un-receded stub and never exercise recede_tip
/// at all. Walling the tip into a corridor only one cell wide removes that randomness: at every
/// step the ONLY unvisited passable neighbor is the next corridor cell, so growth is forced and
/// deterministic regardless of pick_step's RNG. The corridor still zigzags (real turns, not a
/// straight ray) so recede_tip's "follow the actual strand backward through turns" logic (see
/// its doc comment on chords/degree-preference) is still genuinely exercised. Soil is freshly
/// painted (default aux 0, i.e. zero richness -- see `Material::initial_aux`), so the colony's
/// pool never fills and it starves right on schedule once past the grace period.
fn spawn_colony_in_forced_zigzag_corridor(w: &mut World) -> u8 {
    let (start_x, start_y) = (10i32, 60i32);
    let (seg_len, segments) = (20i32, 7); // length 1 + 7*20 = 141 cells, comfortably > grace+recede
    let mut path = vec![(start_x, start_y)];
    let (mut x, mut y) = (start_x, start_y);
    for seg in 0..segments {
        let (dx, dy) = match seg % 4 {
            0 => (1, 0),
            1 => (0, 1),
            2 => (1, 0),
            _ => (0, -1),
        };
        for _ in 0..seg_len {
            x += dx;
            y += dy;
            path.push((x, y));
        }
    }
    let (min_x, max_x) = (path.iter().map(|p| p.0).min().unwrap() - 1, path.iter().map(|p| p.0).max().unwrap() + 1);
    let (min_y, max_y) = (path.iter().map(|p| p.1).min().unwrap() - 1, path.iter().map(|p| p.1).max().unwrap() + 1);
    for yy in min_y..=max_y {
        for xx in min_x..=max_x {
            w.paint(xx, yy, 0, Material::Rock as u8);
        }
    }
    for &(px, py) in &path {
        w.paint(px, py, 0, Material::Soil as u8);
    }
    w.spawn_colony(start_x as usize, start_y as usize)
}

#[test]
fn dieback_reverts_multiple_cells_per_tick() {
    let mut w = World::new(192, 128);
    // No branching, so there's exactly one tip growing one strand -- keeps the cell count
    // delta attributable purely to recede_tip, not try_branch.
    w.params.values[sandgun_core::params::P_MY_BRANCH_CHANCE] = 0.0;
    w.params.values[sandgun_core::params::P_MY_DIEBACK] = 3.0;
    // Width 1: fix 4's perpendicular thickening would otherwise paint ahead of the tip at each
    // turn of the 1-wide corridor, consuming the very next forced cell before the tip gets
    // there (a corner-eating variant of the same cannibalization seen in
    // tip_grows_toward_richer_substrate) and boxing it in. Strand width isn't what this test is
    // about -- it's isolating recede_tip -- so disable it here, same spirit as zeroing
    // P_MY_BRANCH_CHANCE above.
    w.params.values[sandgun_core::params::P_MY_STRAND_WIDTH] = 1.0;
    spawn_colony_in_forced_zigzag_corridor(&mut w);
    // Grace period is 90 growth ticks (age_ticks > 90 is starving); my_grow_countdown starts at
    // 0 so a world step is a growth tick every P_MY_GROWTH_INTERVAL steps. Run exactly the grace
    // period's worth of growth ticks (read dynamically so this stays correct if the interval is
    // retuned) so the colony is one tick shy of starving.
    let interval = w.params.values[sandgun_core::params::P_MY_GROWTH_INTERVAL] as usize;
    for _ in 0..(90 * interval) { w.step(); }
    let before = count_mycelium(&w, (192, 128));
    assert!(
        before >= 4,
        "need a multi-cell strand to exercise multi-cell dieback (only {before} mycelium cells)"
    );
    // One more growth tick: now starving (age_ticks == 91 > 90) -> recede_tip should revert
    // P_MY_DIEBACK (3) cells this tick, not just 1.
    for _ in 0..interval { w.step(); }
    let after = count_mycelium(&w, (192, 128));
    let reverted = before - after;
    assert!(
        reverted >= 3,
        "P_MY_DIEBACK=3 should revert ~3 cells in a single recede tick, only {reverted} reverted"
    );
}

#[test]
fn winding_strand_fully_recedes_no_stubs() {
    let mut w = World::new(192, 128);
    // No branching, so there's exactly one tip/strand -- isolates recede_tip's full-unwind
    // behavior from try_branch.
    w.params.values[sandgun_core::params::P_MY_BRANCH_CHANCE] = 0.0;
    // See the width-1 note in dieback_reverts_multiple_cells_per_tick above: fix 4's
    // perpendicular thickening would otherwise eat the corridor's next forced cell at each turn.
    w.params.values[sandgun_core::params::P_MY_STRAND_WIDTH] = 1.0;
    spawn_colony_in_forced_zigzag_corridor(&mut w);
    // Grow past the grace period so it starts starving, then keep going long enough to fully
    // unwind whatever strand it grew (grace ~90 growth ticks to grow, plus generous budget to
    // recede at the default dieback rate) and for the world to settle. Scaled off the actual
    // growth interval so this stays correct if it's retuned.
    let interval = w.params.values[sandgun_core::params::P_MY_GROWTH_INTERVAL] as usize;
    for _ in 0..(400 * interval) { w.step(); }
    w.step();
    assert_eq!(w.tip_count(), 0, "starved tip should have died once fully receded");
    assert_eq!(
        count_mycelium(&w, (192, 128)),
        0,
        "winding strand should fully unwind -- no leftover dead-mycelium stubs"
    );
    assert_eq!(w.cells_processed, 0, "receded, settled world sleeps");
}

/// A straight, 1-wide, rock-walled soil corridor of exactly `length` cells starting at
/// (10, 60) and running rightward, with the colony's tip's own root-ward growth guaranteed to
/// lay down exactly `length` Mycelium cells over `length` growth ticks (no branching, no
/// perpendicular thickening -- same technique as `spawn_colony_in_forced_zigzag_corridor`).
/// Leaves one Empty gap in the wall directly above `gap_x` so a test can plant a Fire cell
/// there and let it spread into the corridor via the normal ignite_neighbors path.
fn spawn_colony_in_straight_corridor(w: &mut World, length: i32, gap_x: i32) -> u8 {
    let (start_x, start_y) = (10i32, 60i32);
    let (min_x, max_x) = (start_x - 1, start_x + length + 1);
    let (min_y, max_y) = (start_y - 1, start_y + 1);
    for yy in min_y..=max_y {
        for xx in min_x..=max_x {
            w.paint(xx, yy, 0, Material::Rock as u8);
        }
    }
    for i in 0..=length {
        w.paint(start_x + i, start_y, 0, Material::Soil as u8);
    }
    w.paint(gap_x, start_y - 1, 0, Material::Empty as u8);
    w.spawn_colony(start_x as usize, start_y as usize)
}

#[test]
fn recede_continues_past_a_burning_segment() {
    // Regression (M1e cleanup task 4): a burning Mycelium cell's aux holds the fuel countdown,
    // not the colony id, so recede_tip's adjacent_same_colony_mycelium used to treat a burning
    // same-strand neighbor as foreign. A tip receding through a strand whose next segment is on
    // fire died right there, permanently stranding the rest of the strand (root-ward of the
    // fire) as dead Mycelium that no tip would ever revert.
    let mut w = World::new(192, 128);
    // Isolate recede_tip: no branching, single-cell-wide strand, so the grown path is a known
    // straight line (same technique as dieback_reverts_multiple_cells_per_tick /
    // winding_strand_fully_recedes_no_stubs).
    w.params.values[sandgun_core::params::P_MY_BRANCH_CHANCE] = 0.0;
    w.params.values[sandgun_core::params::P_MY_STRAND_WIDTH] = 1.0;
    // Recede several cells per growth tick so the strand fully unwinds within a modest step
    // budget.
    w.params.values[sandgun_core::params::P_MY_DIEBACK] = 5.0;
    // Guarantee ignition on the very first adjacent contact, and give the ignited cell far more
    // fuel than it could possibly need before recede reaches it -- the point of this test is
    // recede walking through a cell that is STILL burning, not a race against its own burnout.
    w.params.values[sandgun_core::params::P_FLAM_MYCELIUM] = 1.0;
    w.params.values[sandgun_core::params::P_FUEL_MYCELIUM] = 500.0;

    let start_x = 10i32;
    let start_y = 60i32;
    let length = 90i32; // matches the 90 growth ticks run below (STARVE_GRACE_TICKS)
    let mid_x = start_x + length / 2;
    let id = spawn_colony_in_straight_corridor(&mut w, length, mid_x);

    // Grow for exactly the grace period's worth of growth ticks -- one tick shy of starving --
    // so the strand is a known straight line from start_x to start_x+length.
    let interval = w.params.values[sandgun_core::params::P_MY_GROWTH_INTERVAL] as usize;
    for _ in 0..(90 * interval) { w.step(); }
    assert_eq!(
        w.get(mid_x as usize, start_y as usize),
        Material::Mycelium,
        "setup: growth should have reached past the midpoint before starving"
    );

    // Ignite the midpoint cell via the normal fire-spread path (not by poking flags directly),
    // same technique as burning_flesh_puffs_non_burning_spores.
    w.paint(mid_x, start_y - 1, 0, Material::Fire as u8);
    for _ in 0..5 {
        w.step();
        if w.cell_flags(mid_x as usize, start_y as usize) & sandgun_core::cell::FLAG_BURNING != 0 {
            break;
        }
    }
    assert_ne!(
        w.cell_flags(mid_x as usize, start_y as usize) & sandgun_core::cell::FLAG_BURNING,
        0,
        "setup: the midpoint cell should have caught fire"
    );
    // Stop the fire from cascading down the rest of the strand -- this test is about recede
    // walking through ONE still-burning cell, not about the whole corridor burning down on its
    // own regardless of recede (P_FLAM_MYCELIUM=1.0 above guaranteed the single ignition; left
    // at 1.0 it would also guarantee ignite_neighbors keeps igniting every other Mycelium cell
    // along the strand every frame, clearing it independent of the bug this test targets).
    w.params.values[sandgun_core::params::P_FLAM_MYCELIUM] = 0.0;

    // One more growth tick (age_ticks now > 90) starts starvation/recede; run long enough for
    // the tip to fully recede -- straight through the still-burning midpoint cell -- and for the
    // world to settle.
    for _ in 0..(300 * interval) { w.step(); }
    w.step();

    assert_eq!(w.tip_count(), 0, "starved tip should have died once fully receded");
    assert_eq!(
        count_mycelium(&w, (192, 128)),
        0,
        "recede should continue past the burning segment and fully unwind the strand -- no \
         permanent stub left behind it"
    );
    assert_eq!(w.cells_processed, 0, "receded, settled world sleeps");
    // Task 1's cell_count accounting must not have been double-decremented by traversing the
    // burning cell (it was already decremented once, at ignition) -- otherwise this would
    // underflow (saturating, so silently wrong rather than panicking) or the colony would reap
    // prematurely mid-strand. With every cell gone, the colony should reap cleanly: 0 cells and
    // no longer present in the colony list at all.
    assert_eq!(w.colony_cell_count(id), 0, "colony should have no cells left after full recede");
    assert_eq!(w.colony_count(), 0, "fully-receded, tipless colony should have been reaped");
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
fn acid_ammo_severs_mycelium_bridge_and_far_side_falls() {
    // Same setup/shape as severed_mycelium_bridge_falls, but severing with Acid ammo instead of
    // Kinetic. inject_blob (the Acid impact path) can overwrite Mycelium, and unlike
    // carve_crater it must also trigger the support check -- otherwise a Rock-anchored bridge
    // severed by acid leaves the far side floating as inert Mycelium forever.
    let mut w = World::new(64, 64);
    w.paint(5, 32, 0, Material::Rock as u8); // anchor
    // a long mycelium bridge from the anchor (x=6, touching the rock) out to a floating far end
    for x in 6..40 { w.paint(x as i32, 32, 0, Material::Mycelium as u8); }
    let particles_before = w.particle_count();
    // fire acid straight down into the middle of the bridge (x=23), splitting it in two
    w.fire(23.5, 27.0, 0.0, 8.0, Ammo::Acid as u8);
    w.step(); // impact + inject_blob + the resulting support check, all this frame
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
    // far side (was disconnected by the acid blob, no anchor within reach) should have fallen away
    for x in 30..39 {
        assert_ne!(
            w.get(x, 32),
            Material::Mycelium,
            "disconnected piece at x={x} should have fallen, not stayed in place"
        );
    }
}

#[test]
fn acid_erosion_of_soil_anchor_strands_mycelium() {
    // update_acid's per-tick erosion (not ammo impact) dissolves cells one at a time. Here the
    // ONLY anchor for a mycelium strand is a single Soil cell; acid sitting on top of it erodes
    // that soil away over several ticks. Once the soil is gone, the whole strand -- never
    // directly touched by the acid -- has lost its only anchor and must be dropped, same as if
    // it had been carved or burned away.
    let mut w = World::new(64, 64);
    // Rock directly under the soil only -- keeps the soil from falling under its own gravity
    // (Soil is a powder) without being itself D4-adjacent to any mycelium cell, so it can't
    // serve as a second anchor once the soil above it is gone.
    w.paint(10, 33, 0, Material::Rock as u8);
    w.paint(10, 32, 0, Material::Soil as u8); // the sole anchor
    for x in 11..30 { w.paint(x as i32, 32, 0, Material::Mycelium as u8); }
    w.paint(10, 31, 0, Material::Acid as u8); // resting directly on top of the soil anchor
    // let the acid eat through the soil anchor (bounded by its own dissolve-charge budget), then
    // give any resulting drop plenty of time to fall and settle
    for _ in 0..900 {
        w.step();
    }
    assert_ne!(w.get(10, 32), Material::Soil, "acid should have dissolved the sole anchor by now");
    for x in 11..29 {
        assert_ne!(
            w.get(x, 32),
            Material::Mycelium,
            "strand at x={x} lost its only anchor to erosion and should have fallen, not stayed"
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

// --- Task 3 (M1e cleanup): coalesced/deduped support-check pass -- world.rs's
// pending_drop_checks + drop_unsupported_pending() replace the old per-removal-site inline
// drop_unsupported_around() calls (still called from acid dissolve, burnout, carve_crater,
// inject_blob -- now all via the pending queue instead) with ONE pass sharing a single `visited`
// set per step, so overlapping removals from the same event don't each re-flood the same
// connected group. The tests above (severed_mycelium_bridge_falls et al.) already cover the
// single-removal-site case unchanged; this test's job is specifically the OVERLAP case: two
// separate removal sites landing in `pending_drop_checks` in the very same step, both reaching
// into the SAME connected, unsupported mass.

#[test]
fn overlapping_acid_removals_flood_each_group_once() {
    // A solid floating slab (no Rock/Soil anywhere nearby, so the whole thing is unsupported)
    // with two acid cells embedded well inside it. P_ACID_ETCH=1.0 makes acid's dissolve roll
    // succeed unconditionally, and every one of an embedded acid cell's 4 neighbors is Mycelium,
    // so whichever of the 4 directions update_acid's own per-tick coin flip picks, it *will*
    // dissolve a Mycelium neighbor on the very first tick both acid cells are processed -- i.e.
    // deterministically, both removal sites are queued in `pending_drop_checks` in the SAME
    // step(), and both fall within the same connected slab. That's the overlap this test needs,
    // with no reliance on timing luck.
    let mut w = World::new(64, 64);
    for x in 0..64 { w.paint(x as i32, 63, 0, Material::Rock as u8); } // floor to catch debris
    w.params.values[sandgun_core::params::P_ACID_ETCH] = 1.0;

    const X0: i32 = 10;
    const X1: i32 = 30; // inclusive
    const Y0: i32 = 28;
    const Y1: i32 = 32; // inclusive
    for x in X0..=X1 {
        for y in Y0..=Y1 {
            w.paint(x, y, 0, Material::Mycelium as u8);
        }
    }
    // Both interior (all 4 neighbors inside the slab), far enough apart that they seed two
    // distinct search windows, yet both reach the one connected slab (a single 21x5 mass, minus
    // two single-cell holes once dissolved, is still one 8-connected group). Note: the two acid
    // cells themselves are left behind (spent charge, not fully consumed) sitting right where the
    // slab used to be, so a cell or two of debris directly above one can fall one step and
    // immediately re-settle as solid Mycelium on top of it -- that's ordinary particle/solid
    // interaction, not a drop-check bug, so this test checks aggregate particle count and
    // eventual settling rather than "every original cell is now empty".
    w.paint(15, 30, 0, Material::Acid as u8);
    w.paint(25, 30, 0, Material::Acid as u8);

    let slab_cells = ((X1 - X0 + 1) * (Y1 - Y0 + 1)) as usize;
    let particles_before = w.particle_count();
    w.step(); // both acid dissolves + the one coalesced support-check pass, all this frame
    // Both acid cells dissolve one neighbor each (2 cells removed with no particle -- a direct
    // acid dissolve, like a burnout, just clears the cell) and then, since the deferred flood
    // sees the whole slab still intact until the pass runs at the end of THIS SAME step, the rest
    // of the connected mass (slab_cells - 2 embedded acid cells - 2 dissolved cells) drops as
    // particles in one shot. Assert "most of the slab", not an exact count: which two interior
    // cells acid happens to pick is randomized (bounded only to be one of the 4 Mycelium
    // neighbors), so the exact remainder varies by at most a cell or two depending on the pick.
    assert!(
        w.particle_count() >= particles_before + slab_cells - 6,
        "the whole unsupported slab should have detached into falling particles this step (got {} particles, slab had {slab_cells} cells)",
        w.particle_count() - particles_before
    );
    for _ in 0..500 { w.step(); } // let debris fall/settle
    assert_eq!(w.particle_count(), 0, "dropped debris should have landed by now");
    w.step();
    assert_eq!(w.cells_processed, 0, "world should settle once the coalesced drop resolves");
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
    // The brief's literal setup (spawn_colony in a plain, all-open-air 64x64 world) does NOT
    // actually exercise reaping here: with the default params (P_MY_MAX_AIR_REACH=3,
    // P_MY_STRAND_WIDTH=2), a tip with no soil anywhere always hits the air-reach cap and dies
    // via the "boxed in, pick_step found nowhere to go" path (same one exercised by
    // tip_count_tracks_live_tips) a few growth ticks in -- well before STARVE_GRACE_TICKS(90)
    // ever makes it eligible for the starvation-recede path (recede_tip). Since only recede_tip
    // (or an explicit removal like fire/acid/carve) ever decrements cell_count, such a colony
    // permanently keeps ~7 stub Mycelium cells in the grid (verified empirically) and its
    // cell_count never reaches 0 -- so it correctly must NOT be reaped, or a recycled id would
    // silently inherit ownership of those real, still-visible leftover cells (exactly the
    // mislabeling bug this task exists to prevent).
    //
    // To genuinely exercise "id recycled once a colony is truly gone", use the same
    // corridor-starvation setup as `winding_strand_fully_recedes_no_stubs` below, which already
    // proves the strand fully unwinds to zero mycelium cells left in the grid (a real cell_count
    // of 0, not just a dead-but-still-present stub).
    let mut w = World::new(192, 128);
    w.params.values[sandgun_core::params::P_MY_BRANCH_CHANCE] = 0.0;
    w.params.values[sandgun_core::params::P_MY_STRAND_WIDTH] = 1.0;
    let a = spawn_colony_in_forced_zigzag_corridor(&mut w);
    let interval = w.params.values[sandgun_core::params::P_MY_GROWTH_INTERVAL] as usize;
    for _ in 0..(400 * interval) { w.step(); }
    w.step();
    assert_eq!(w.tip_count(), 0, "setup: starved tip should have died once fully receded");
    assert_eq!(
        count_mycelium(&w, (192, 128)),
        0,
        "setup: colony should have fully unwound with no stub cells left"
    );
    let b = w.spawn_colony(50, 50);
    assert_eq!(a, b, "the reaped colony's id is reused for the next colony");
}

#[test]
fn colony_with_live_cells_keeps_its_id_until_cells_gone() {
    // A colony that still has mycelium cells in the grid must NOT have its id reused, or a new
    // colony would inherit ownership of the old cells.
    // World dims must be multiples of CHUNK (64); the brief's 96x96 example doesn't satisfy
    // that (a pre-existing World::new invariant, unrelated to this task -- same workaround as
    // well_fed_colony_branches_up_to_the_cap above), so this uses 128x128 with the same soil
    // field and colony coordinates (all well within bounds either way).
    let mut w = World::new(128, 128);
    for x in 0..96 { for y in 60..70 { w.paint(x as i32, y as i32, 0, Material::Soil as u8); w.set_soil_richness(x, y, 200); } }
    let a = w.spawn_colony(48, 65);
    for _ in 0..400 { w.step(); } // a grows a real network in the soil
    // a is alive with cells; the next spawn must get a DIFFERENT id
    let b = w.spawn_colony(10, 65);
    assert_ne!(a, b, "an id with live cells is not recycled");
}

#[test]
fn full_lifecycle_world_still_sleeps_after_settling() {
    // M1e task 7 -- the milestone's kill-criterion regression test. Colonies (worldgen-seeded, so
    // they grow, eat, branch, fruit, and recede live), the avatar, a fired projectile, AND free
    // particles must all be able to be live in the SAME world at once, and once everything
    // settles the world must go fully quiet: cells_processed == 0, no live tips left, no
    // mushrooms left standing. This combines the colony-only guard
    // (generated_world_fully_settles, tests/worldgen.rs) with the entity-only guard
    // (all_entity_kinds_settle_to_sleep, tests/avatar.rs) into one scene, so a future regression
    // that only breaks when everything is live simultaneously (e.g. an entity that keeps nudging
    // a chunk awake near a growing/receding colony) gets caught.
    let mut w = World::new(256, 192);
    worldgen::generate(&mut w, 7);
    assert!(w.colony_count() > 0, "worldgen should seed living colonies");

    // Avatar, projectile, and particles are all dropped into open air well above the terrain:
    // worldgen's surface heightline (worldgen::generate step 1) is clamped to at least h/5, so
    // every column has open air from y=0 to at least y=38 in this 192-tall world -- y<=20 below
    // is guaranteed clear of terrain regardless of x.
    w.spawn_avatar(40.0, 10.0);
    w.fire(120.0, 10.0, 6.0, 0.0, Ammo::Kinetic as u8);
    w.spawn_particle(60.0, 15.0, 1.0, 0.0, Material::Sand as u8);
    w.spawn_particle(90.0, 20.0, -1.0, 0.0, Material::Sand as u8);
    w.spawn_particle(200.0, 12.0, 0.5, -0.5, Material::Sand as u8);

    // Generous budget: colony-only settling alone takes ~4000 steps for this seed (see
    // generated_world_fully_settles); entity-only settling alone takes a few hundred. 40000 gives
    // wide margin without weakening the assertions below -- they are exact, not approximate.
    for _ in 0..40_000 {
        w.step();
    }
    w.step();

    assert_eq!(w.tip_count(), 0, "all mycelium growth must finish (grow to completion or fully recede)");
    assert_eq!(w.mushroom_len(), 0, "all mushrooms must finish (fruit, decay, or be consumed)");
    assert_eq!(
        w.cells_processed, 0,
        "with colonies, avatar, a projectile, and particles all live at once, the world must fully settle to sleep"
    );
}

/// Box a colony's root cell in on all 8 sides with Rock, so its single tip finds nowhere to
/// step (pick_step only ever grows into Empty/Soil) and dies on the very first growth tick,
/// leaving exactly one real Mycelium cell (the root) in the grid.
fn spawn_rock_boxed_colony(w: &mut World, x: usize, y: usize) -> u8 {
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            if dx == 0 && dy == 0 { continue; }
            w.paint(x as i32 + dx, y as i32 + dy, 0, Material::Rock as u8);
        }
    }
    w.spawn_colony(x, y)
}

#[test]
fn painting_over_a_colony_cell_lets_it_reap() {
    // M1e cleanup task 1 review: World::paint unconditionally overwrites Cell::new(...)
    // regardless of prior contents, so erasing/painting over a live colony-owned Mycelium cell
    // must decrement that colony's cell_count -- otherwise a boxed-in colony whose tip already
    // died (leaving only its root cell) can never reach cell_count == 0, so it leaks: never
    // reaped, its id never recycled.
    let mut w = World::new(128, 128);
    let (bx, by) = (20usize, 20usize);
    let boxed_id = spawn_rock_boxed_colony(&mut w, bx, by);

    // A second, far-away colony in rich soil that keeps at least one tip alive for a good long
    // while. grow_mycelium's own reap sweep (the `colonies.retain` in mycelium.rs) only runs
    // when it does real work -- it no-ops entirely once there are no live tips anywhere and no
    // mushrooms mid-fruit/decay. Without a live tip elsewhere, the boxed colony's own tip dying
    // (below) would be the LAST time that sweep ever ran, so painting over its root afterward
    // would never actually get swept into a reap. This keep-alive colony is what lets later
    // steps still perform the reap check.
    for x in 60..120 { for y in 80..110 { w.paint(x as i32, y as i32, 0, Material::Soil as u8); w.set_soil_richness(x, y, 200); } }
    w.spawn_colony(90, 95);

    let interval = (w.params.values[sandgun_core::params::P_MY_GROWTH_INTERVAL] as usize).max(1);
    for _ in 0..(3 * interval) { w.step(); }

    // Setup check: the boxed colony's tip died leaving just its root, a real live cell -- not
    // yet reaped.
    assert_eq!(w.colony_tip_count(boxed_id), 0, "setup: boxed colony's tip should have died, boxed in by Rock");
    assert_eq!(w.colony_cell_count(boxed_id), 1, "setup: only the root cell should remain");
    assert_eq!(w.get(bx, by), Material::Mycelium, "setup: root cell should still be Mycelium");

    // Erase the colony's only remaining cell with the player's brush (material-agnostic: Sand
    // here, but the bug applies identically to Empty/erase or any other material).
    w.paint(bx as i32, by as i32, 0, Material::Sand as u8);
    assert_eq!(w.colony_cell_count(boxed_id), 0, "paint must decrement the overwritten colony's cell_count");

    for _ in 0..(5 * interval) { w.step(); }

    // The boxed colony must actually be reaped (removed from the colony list, not just
    // "not alive") -- proven by its id being recycled by the next spawn_colony.
    let recycled = w.spawn_colony(5, 5);
    assert_eq!(recycled, boxed_id, "a fully cell-less, tipless colony's id must be recycled");
}

#[test]
fn spore_onto_existing_colony_cell_transfers_ownership_cleanly() {
    // M1e cleanup task 1 review: spawn_colony overwrites the root cell unconditionally too.
    // Firing Spore ammo (which calls spawn_colony) onto an existing colony's live cell must
    // decrement the OLD owner's cell_count, or that cell is silently double-owned: the new
    // colony's grid cell counts against neither colony correctly and the old colony leaks.
    let mut w = World::new(128, 128);

    // Keep-alive colony, spawned FIRST (before `a` and `b` below) and far away: a's tip dies
    // boxed-in on the very first growth tick below, and since a's cell_count is already 0 at
    // that point (the transfer decrements it immediately), a gets reaped in that SAME growth
    // tick -- before this keep-alive colony would otherwise be spawned. Spawning it first
    // ensures it claims its own fresh id rather than stealing a's just-freed one, so the final
    // "id recycled" assertion below unambiguously checks the (5,5) spawn against `a`.
    for x in 60..120 { for y in 80..110 { w.paint(x as i32, y as i32, 0, Material::Soil as u8); w.set_soil_richness(x, y, 200); } }
    w.spawn_colony(90, 95);

    let (bx, by) = (20usize, 20usize);
    let a = spawn_rock_boxed_colony(&mut w, bx, by);
    assert_eq!(w.colony_cell_count(a), 1, "setup: a's root is its only cell");

    // Simulate Spore ammo landing exactly on a's live root cell before a's tip has had any
    // chance to grow or die -- the transfer must decrement a's cell_count regardless.
    let b = spawn_rock_boxed_colony(&mut w, bx, by);
    assert_ne!(a, b, "the new colony must get a fresh id, not reuse a's (a still has other state live)");
    assert_eq!(w.colony_cell_count(a), 0, "a's transferred cell must no longer count toward a");
    assert_eq!(w.colony_cell_count(b), 1, "b now owns the cell it just claimed");
    assert_eq!(w.get(bx, by), Material::Mycelium);

    let interval = (w.params.values[sandgun_core::params::P_MY_GROWTH_INTERVAL] as usize).max(1);
    // Both tips sit on the same boxed-in cell and die on the same first growth tick (no double
    // free / no panic), leaving a reap-able (0 cells) and b holding its one real cell.
    for _ in 0..(3 * interval) { w.step(); }
    assert_eq!(w.colony_tip_count(a), 0);
    assert_eq!(w.colony_tip_count(b), 0);
    assert_eq!(w.colony_cell_count(a), 0, "a has no cells and must never regain any");
    assert_eq!(w.colony_cell_count(b), 1, "b's own root cell is untouched by a's bookkeeping");

    // a is now cell-less and tip-less. Proving it was actually REAPED (not just marked dead)
    // requires its id to come back out of a later spawn_colony -- b's one real cell means b
    // correctly must NOT be recycled while that cell remains.
    let recycled = w.spawn_colony(5, 5);
    assert_eq!(recycled, a, "a's id is recycled once it has neither tips nor cells left");
}

// --- M1e cleanup task 2: concurrent-colony cap + Spore ammo tolerates a full table ---

#[test]
fn spore_ammo_at_colony_cap_is_a_noop() {
    // Set the soft cap low and fill it, then fire Spore ammo into a scene where a landing site
    // (soil) genuinely exists nearby -- this must still be a no-op at the cap: spawn_colony
    // returns 0 (Task 1), and the caller must not create a tip/cell for colony 0.
    let mut w = World::new(64, 64);
    w.params.values[sandgun_core::params::P_MY_MAX_COLONIES] = 2.0;

    for x in 0..64 {
        w.paint(x, 41, 0, Material::Rock as u8); // floor so soil can't avalanche
        for y in 30..40 {
            w.paint(x, y, 0, Material::Soil as u8);
        }
    }
    for x in 0..64usize {
        for y in 30..40usize {
            w.set_soil_richness(x, y, 150);
        }
    }

    w.spawn_colony(5, 5);
    w.spawn_colony(55, 5);
    assert_eq!(w.colony_count(), 2, "setup: colony table is at the cap");
    let tips_before = w.tip_count();

    w.fire(5.0, 35.0, 12.0, 0.0, Ammo::Spore as u8); // fired into the soil band, same as spore_ammo_plants_a_growing_colony
    w.step();

    assert_eq!(w.colony_count(), 2, "colony count must never exceed the concurrent cap");
    assert_eq!(
        w.tip_count(),
        tips_before,
        "no tip should be created for a colony that couldn't be allocated (id 0)"
    );
}

#[test]
fn spore_ammo_plants_on_substrate_not_air() {
    // Fire Spore ammo so its impact point is in open air (a thin Rock wall stops the round a
    // few rows above a soil patch) rather than directly on/adjacent to substrate. The old
    // behavior (spawn_colony straight at the impact point) would plant a floating stub with no
    // Soil neighbor at all. The fix must instead search the nearby neighborhood and plant on
    // Soil or an Empty cell adjacent to Soil.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 41, 0, Material::Rock as u8); // floor so soil can't avalanche
        for y in 40..41 {
            w.paint(x, y, 0, Material::Soil as u8);
        }
    }
    for x in 0..64usize {
        w.set_soil_richness(x, 40, 150);
    }
    // A single Rock cell in open air, 3 rows above the soil band -- the round impacts this,
    // NOT the soil, so the naive impact point is a floating air cell.
    w.paint(30, 37, 0, Material::Rock as u8);

    assert_eq!(w.colony_count(), 0, "setup: no colonies yet");
    // vx is large enough to cross the full x=10 -> x=30 gap within a single w.step() call's
    // ray-march (each step() only advances a projectile by roughly vx cells of substeps).
    w.fire(10.0, 37.0, 25.0, 0.0, Ammo::Spore as u8); // flies straight at the Rock cell at (30,37)
    w.step();

    assert!(w.colony_count() > 0, "a landing site (soil is within the search radius) should have been found and planted");

    // Find the planted root cell and confirm it sits ON Soil or directly ADJACENT to Soil --
    // never a bare floating cell with no substrate neighbor at all.
    let mut root: Option<(usize, usize)> = None;
    for x in 0..64usize {
        for y in 0..64usize {
            if w.get(x, y) == Material::Mycelium {
                root = Some((x, y));
            }
        }
    }
    let (rx, ry) = root.expect("a Mycelium root cell should have been planted");
    let on_or_adjacent_to_soil = {
        let mut found = false;
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let (nx, ny) = (rx as i32 + dx, ry as i32 + dy);
                if nx < 0 || ny < 0 || nx as usize >= 64 || ny as usize >= 64 {
                    continue;
                }
                if w.get(nx as usize, ny as usize) == Material::Soil {
                    found = true;
                }
            }
        }
        found
    };
    assert!(
        on_or_adjacent_to_soil,
        "planted colony root at ({rx},{ry}) must sit on or adjacent to Soil, not floating in open air"
    );
    assert_ne!((rx, ry), (30, 37), "must not plant at the bare air impact point itself");
}

// --- M1e cleanup final review: recycled-id spawns must respect a lowered cap ---

#[test]
fn lowering_colony_cap_mid_session_is_respected() {
    // alloc_colony_id's cap check used to run ONLY when free_colony_ids was empty:
    //   if self.free_colony_ids.is_empty() && self.colonies.len() >= cap { return None; }
    // so any recycled id bypassed the cap entirely. That's harmless under a static cap, but
    // P_MY_MAX_COLONIES is hot-tunable (set_param) -- lowering it mid-session while colonies
    // churn (spawn/reap/spawn) must still be respected via the free-list path, not just the
    // fresh-id path.
    let mut w = World::new(128, 128);

    // Keep-alive colony in rich soil, far from everything below, so it keeps at least one tip
    // growing for the whole test. grow_mycelium (and its colonies.retain reap sweep) is a no-op
    // whenever there are no live tips anywhere -- without this, once a/b/c's tips all died the
    // reap sweep would never run again and free_colony_ids would never actually gain entries.
    for x in 90..120 { w.paint(x, 111, 0, Material::Rock as u8); } // floor so soil can't avalanche away
    for x in 90..120 { for y in 90..110 { w.paint(x as i32, y as i32, 0, Material::Soil as u8); w.set_soil_richness(x, y, 200); } }
    w.spawn_colony(100, 95);

    // Three colonies boxed solid by Rock, far apart so their boxes can't interfere with each
    // other or with the keep-alive colony's soil patch. Each tip dies on its very first growth
    // tick (boxed in, pick_step finds nowhere to go), leaving just a root cell.
    let a = spawn_rock_boxed_colony(&mut w, 10, 10);
    let b = spawn_rock_boxed_colony(&mut w, 10, 30);
    let c = spawn_rock_boxed_colony(&mut w, 10, 50);

    let interval = (w.params.values[sandgun_core::params::P_MY_GROWTH_INTERVAL] as usize).max(1);
    for _ in 0..(3 * interval) { w.step(); }
    assert_eq!(w.colony_tip_count(a), 0, "setup: a's tip should have died, boxed in by Rock");
    assert_eq!(w.colony_tip_count(b), 0, "setup: b's tip should have died, boxed in by Rock");
    assert_eq!(w.colony_tip_count(c), 0, "setup: c's tip should have died, boxed in by Rock");

    // Erase each boxed colony's root cell so cell_count drops to 0 -- the last condition for
    // the reap sweep to actually recycle their ids.
    w.paint(10, 10, 0, Material::Sand as u8);
    w.paint(10, 30, 0, Material::Sand as u8);
    w.paint(10, 50, 0, Material::Sand as u8);
    for _ in 0..(3 * interval) { w.step(); }
    assert_eq!(w.colony_cell_count(a), 0, "setup: a should be fully cell-less");
    assert_eq!(w.colony_cell_count(b), 0, "setup: b should be fully cell-less");
    assert_eq!(w.colony_cell_count(c), 0, "setup: c should be fully cell-less");
    assert_eq!(w.colony_count(), 1, "setup: only the keep-alive colony remains live -- a/b/c fully reaped, ids on the free list");

    // Now lower the cap to exactly the current (live) colony count. Any further spawn must be
    // rejected, including ones that would only pull a RECYCLED id from the free list.
    w.set_param(sandgun_core::params::P_MY_MAX_COLONIES as u32, 1.0);

    for i in 0..3 {
        let id = w.spawn_colony(60, 10 + i * 20);
        assert_eq!(id, 0, "spawn at the lowered cap must be rejected even via a recycled id");
        assert_eq!(w.colony_count(), 1, "colony count must never exceed the lowered cap, even via free-list reuse");
    }
}

