use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn particle_falls_and_settles_on_the_floor() {
    let mut w = World::new(64, 64);
    // rock floor at y=60
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8);
    }
    w.spawn_particle(10.0, 5.0, 0.0, 0.0, Material::Sand as u8);
    assert_eq!(w.particle_count(), 1);
    for _ in 0..400 {
        w.step();
    }
    assert_eq!(w.particle_count(), 0, "particle must resettle, not fly forever");
    // it should have become a grid cell resting on the floor (row 59, just above the sand)
    assert_eq!(w.get(10, 59), Material::Sand, "particle resettled onto the floor");
}

#[test]
fn particle_with_sideways_velocity_lands_offset() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8);
    }
    w.spawn_particle(10.0, 5.0, 3.0, 0.0, Material::Sand as u8); // flung right
    for _ in 0..400 {
        w.step();
    }
    assert_eq!(w.particle_count(), 0);
    // landed to the right of x=10
    let landed = (11..64).any(|x| w.get(x, 59) == Material::Sand);
    assert!(landed, "a particle flung sideways should land to the right of its origin");
}

#[test]
fn particle_leaving_the_world_is_dropped() {
    let mut w = World::new(64, 64);
    w.spawn_particle(32.0, 5.0, 0.0, -50.0, Material::Sand as u8); // flung up and out
    for _ in 0..20 {
        w.step();
    }
    assert_eq!(w.particle_count(), 0, "particle that exits the world is dropped, not kept");
}

#[test]
fn particles_do_not_keep_the_world_awake_forever() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8);
    }
    w.spawn_particle(10.0, 5.0, 0.0, 0.0, Material::Sand as u8);
    for _ in 0..400 {
        w.step();
    }
    w.step();
    assert_eq!(w.particle_count(), 0);
    assert_eq!(w.cells_processed, 0, "once particles resettle and the grid settles, work is 0");
}
