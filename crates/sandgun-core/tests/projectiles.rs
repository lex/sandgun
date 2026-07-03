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
