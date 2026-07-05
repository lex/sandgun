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
        w.paint(20, y, 0, Material::Rock as u8); // 1-cell-thick wall
    }
    // very fast: 20 cells/frame would tunnel a 1-cell wall without ray-marching.
    // NOTE: brief's original numbers (wall at x=30, fire from x=2.0) put the wall
    // out of reach of a single frame's travel (2.0 + 20 substeps of 1.0 = 22.0 max,
    // never reaching x=30), so the projectile survived the one step() call and the
    // test could never pass. Moved the wall to x=20 (within the 20-cell single-frame
    // reach) so the naive non-ray-marched landing cell (floor(2.0+20.0)=22) would
    // land past the wall, making this an actual tunneling scenario.
    w.fire(2.0, 32.0, 20.0, 0.0, Ammo::Kinetic as u8);
    w.step();
    // the wall must still be intact directly behind where it should have stopped;
    // with a stub impact (no carve), the wall is fully intact and the projectile is gone
    assert_eq!(w.projectile_count(), 0, "projectile resolved its impact on the wall");
    assert_eq!(w.get(20, 32), Material::Rock, "thin wall not tunneled (stub impact carves nothing)");
    // nothing past the wall was disturbed
    assert!((21..64).all(|x| w.get(x, 32) == Material::Empty));
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

#[test]
fn fire_with_zero_velocity_is_rejected() {
    let mut w = World::new(64, 64);
    w.fire(32.0, 32.0, 0.0, 0.0, Ammo::Kinetic as u8);
    assert_eq!(w.projectile_count(), 0, "zero-velocity projectile must not be spawned");
}

#[test]
fn fire_with_nan_velocity_is_rejected() {
    let mut w = World::new(64, 64);
    w.fire(32.0, 32.0, f32::NAN, 0.0, Ammo::Kinetic as u8);
    assert_eq!(
        w.projectile_count(),
        0,
        "NaN velocity projectile must not be spawned"
    );

    let mut w2 = World::new(64, 64);
    w2.fire(32.0, 32.0, 0.0, f32::NAN, Ammo::Kinetic as u8);
    assert_eq!(
        w2.projectile_count(),
        0,
        "NaN velocity projectile must not be spawned"
    );
}

#[test]
fn fire_with_infinite_velocity_is_rejected() {
    let mut w = World::new(64, 64);
    w.fire(32.0, 32.0, f32::INFINITY, 0.0, Ammo::Kinetic as u8);
    assert_eq!(
        w.projectile_count(),
        0,
        "infinite velocity projectile must not be spawned"
    );

    let mut w2 = World::new(64, 64);
    w2.fire(32.0, 32.0, 0.0, f32::NEG_INFINITY, Ammo::Kinetic as u8);
    assert_eq!(
        w2.projectile_count(),
        0,
        "infinite velocity projectile must not be spawned"
    );
}

#[test]
fn fire_with_extreme_velocity_is_clamped_and_completes() {
    let mut w = World::new(64, 64);
    // Create walls at x=10 and x=54 to bound the projectile
    for y in 0..64 {
        w.paint(10, y, 0, Material::Rock as u8);
        w.paint(54, y, 0, Material::Rock as u8);
    }
    // Fire with extreme velocity (1000.0)
    w.fire(32.0, 32.0, 1000.0, 0.0, Ammo::Kinetic as u8);
    assert_eq!(w.projectile_count(), 1);

    // Step should complete without hanging/infinite loop
    // and projectile should hit the wall eventually
    for _ in 0..100 {
        w.step();
        if w.projectile_count() == 0 {
            break;
        }
    }
    assert_eq!(
        w.projectile_count(),
        0,
        "high-velocity projectile must eventually hit a wall and terminate"
    );
}

#[test]
fn normal_velocity_projectiles_still_work() {
    let mut w = World::new(64, 64);
    for y in 0..64 {
        w.paint(40, y, 0, Material::Rock as u8);
    }
    w.fire(5.0, 32.0, 6.0, 0.0, Ammo::Kinetic as u8);
    assert_eq!(w.projectile_count(), 1);
    w.step();
    // Should have moved and still be alive
    assert_eq!(w.projectile_count(), 1);
    for _ in 0..30 {
        w.step();
    }
    assert_eq!(
        w.projectile_count(),
        0,
        "normal projectile eventually hits wall"
    );
}

#[test]
fn projectile_passes_through_fire_and_hits_target() {
    let mut w = World::new(64, 64);
    // Disable fire flicker so the flame cells stay put deterministically in the flight
    // path instead of randomly drifting upward out of the way.
    w.params.values[sandgun_core::params::P_FIRE_FLICKER] = 0.0;

    // A solid target column, far enough past the fire that a stray blast radius from a
    // premature detonation on the fire can't reach it.
    for y in 0..64 {
        w.paint(50, y, 0, Material::Soil as u8);
    }

    // A few cells of leftover Fire sitting directly in the flight path, between the
    // spawn point and the target -- e.g. flames left behind by a prior shot.
    for x in 20..24 {
        w.paint(x, 32, 0, Material::Fire as u8);
    }

    w.fire(5.0, 32.0, 6.0, 0.0, Ammo::Kinetic as u8);
    assert_eq!(w.projectile_count(), 1);
    // Stop stepping as soon as the projectile resolves: Soil is a powder, so further
    // steps would let the column above an impact crater cave back in and refill the
    // hole, masking the very evidence (a cleared cell) this test checks for.
    for _ in 0..30 {
        w.step();
        if w.projectile_count() == 0 {
            break;
        }
    }

    assert_eq!(
        w.projectile_count(),
        0,
        "projectile must eventually impact something"
    );
    // Kinetic impact carves whatever it hits (except Rock) down to Empty, so the impact
    // site is easy to tell apart from an undisturbed cell.
    assert_eq!(
        w.get(50, 32),
        Material::Empty,
        "projectile must reach and carve the target -- it must not detonate early on the fire"
    );
    assert_eq!(
        w.get(20, 32),
        Material::Fire,
        "the fire in the flight path must be left undisturbed, not treated as the impact site"
    );
}
