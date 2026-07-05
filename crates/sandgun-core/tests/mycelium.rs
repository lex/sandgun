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
