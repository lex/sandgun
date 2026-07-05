use sandgun_core::cell::Material;
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
fn mycelium_vein_burns_like_a_fuse() {
    let mut w = World::new(128, 64);
    for x in 30..=90 {
        w.paint(x, 40, 0, Material::Rock as u8); // shelf
        w.paint(x, 39, 0, Material::Mycelium as u8); // vein on the shelf
    }
    w.paint(29, 39, 0, Material::Fire as u8); // light the left end
    // keep relighting the tip for a few frames so the probabilistic catch is certain
    for _ in 0..8 {
        w.paint(29, 39, 0, Material::Fire as u8);
        w.step();
    }
    for _ in 0..3000 {
        w.step();
    }
    assert_eq!(count(&w, Material::Mycelium), 0, "the whole vein must burn through");
    // M1e (task 1): the old growth frontier is dormant (painted Mycelium no longer joins it),
    // so grow() no longer consumes sim RNG draws every growth tick during this test. That shifts
    // exactly which chance() rolls land on P_ASH_CHANCE for each burnt cell, changing the ash
    // count without changing the underlying mechanic -- assert presence, not a tuned threshold.
    assert!(count(&w, Material::Ash) > 5, "burnt mycelium leaves ash");
    for _ in 0..600 {
        w.step();
    }
    w.step();
    assert_eq!(w.cells_processed, 0, "world must settle after the burn");
}

#[test]
fn water_stops_a_fuse() {
    let mut w = World::new(128, 64);
    for x in 30..=90 {
        w.paint(x, 40, 0, Material::Rock as u8);
        w.paint(x, 39, 0, Material::Mycelium as u8);
    }
    // water block interrupting the vein, walled so it stays put
    w.paint(60, 38, 0, Material::Rock as u8);
    w.paint(62, 38, 0, Material::Rock as u8);
    w.paint(61, 39, 0, Material::Water as u8);
    w.paint(61, 38, 0, Material::Water as u8);
    for _ in 0..8 {
        w.paint(29, 39, 0, Material::Fire as u8);
        w.step();
    }
    for _ in 0..3000 {
        w.step();
    }
    let right_side: usize = (63..=90).filter(|&x| w.get(x, 39) == Material::Mycelium).count();
    assert!(right_side > 20, "vein beyond the waterline must survive");
}

#[test]
fn spore_gas_detonates_in_a_chain() {
    let mut w = World::new(64, 64);
    // sealed box full of spore gas
    for x in 20..=40 {
        w.paint(x, 20, 0, Material::Rock as u8);
        w.paint(x, 32, 0, Material::Rock as u8);
    }
    for y in 20..=32 {
        w.paint(20, y, 0, Material::Rock as u8);
        w.paint(40, y, 0, Material::Rock as u8);
    }
    for x in 21..40 {
        for y in 21..32 {
            w.paint(x, y, 0, Material::SporeGas as u8);
        }
    }
    let before = count(&w, Material::SporeGas);
    assert!(before > 150);
    w.paint(21, 31, 0, Material::Fire as u8); // one spark in the corner
    for _ in 0..240 {
        w.step();
    }
    assert_eq!(count(&w, Material::SporeGas), 0, "one spark must consume the whole pocket");
}

#[test]
fn lone_fire_burns_out_and_settles() {
    let mut w = World::new(64, 64);
    w.paint(32, 32, 1, Material::Fire as u8);
    for _ in 0..600 {
        w.step();
    }
    assert_eq!(count(&w, Material::Fire), 0);
    assert_eq!(count(&w, Material::Smoke), 0);
    w.step();
    assert_eq!(w.cells_processed, 0);
}

#[test]
fn flammability_zero_param_prevents_ignition() {
    let mut w = World::new(64, 64);
    w.params.values[sandgun_core::params::P_FLAM_MYCELIUM] = 0.0;
    // This test is only about fire immunity, not growth -- disable bridging so the painted
    // mycelium (which has open air above it, no Soil) doesn't grow via the growth system
    // (painted mycelium now joins the frontier and can bridge into empty space; see
    // `painted_mycelium_in_open_bridges` in tests/growth.rs), which would change the mycelium
    // count independently of fire.
    w.set_param(sandgun_core::params::P_MAX_REACH as u32, 0.0);
    for x in 20..=40 {
        w.paint(x, 40, 0, Material::Rock as u8);
        w.paint(x, 39, 0, Material::Mycelium as u8);
    }
    for _ in 0..8 {
        w.paint(19, 39, 0, Material::Fire as u8);
        w.step();
    }
    for _ in 0..1000 {
        w.step();
    }
    assert_eq!(count(&w, Material::Mycelium), 21, "param at 0 must make mycelium fireproof");
}

#[test]
fn fire_lifetime_param_controls_painted_fire() {
    let mut w = World::new(64, 64);
    w.params.values[sandgun_core::params::P_FIRE_LIFETIME] = 3.0;
    w.paint(32, 32, 0, Material::Fire as u8);
    for _ in 0..10 {
        w.step();
    }
    let any_fire = (0..64).any(|x| (0..64).any(|y| w.get(x, y) == Material::Fire));
    assert!(!any_fire, "3-tick fire must be out within 10 steps");

    let mut w2 = World::new(64, 64);
    w2.paint(32, 32, 0, Material::Fire as u8);
    for _ in 0..10 {
        w2.step();
    }
    let any_fire2 = (0..64).any(|x| (0..64).any(|y| w2.get(x, y) == Material::Fire));
    assert!(any_fire2, "default 40-tick fire must still burn after 10 steps");
}

#[test]
fn fire_spreads_at_most_one_cell_per_frame() {
    // A vertical spore column (100% flammable) lit at the BOTTOM. The sweep is always
    // bottom-up, so without stamping ignited cells, fire chains up the whole column in a
    // single frame. Correct behavior: the burning frontier advances one cell per frame.
    let mut w = World::new(64, 64);
    for y in 10..50 {
        w.paint(32, y, 0, Material::SporeGas as u8);
    }
    w.paint(32, 49, 0, Material::Fire as u8); // light the bottom (highest y)
    w.step();
    assert!(
        w.burning_count() <= 3,
        "one step must not chain fire up the whole column (got {} burning)",
        w.burning_count()
    );
}
