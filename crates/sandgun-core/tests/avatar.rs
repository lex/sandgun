use sandgun_core::cell::Material;
use sandgun_core::projectile::Ammo;
use sandgun_core::world::World;

#[test]
fn avatar_falls_and_rests_on_the_floor() {
    // Tight tolerance on purpose: the avatar's y is fractional almost every frame
    // (gravity accumulates in 0.3px steps), so a loose tolerance here would hide a
    // collision AABB that misses the trailing fractional cell and lets the avatar
    // sink up to ~1px into the floor.
    for spawn_x in [30.0, 30.1, 30.25, 30.5, 30.75, 30.9] {
        let mut w = World::new(64, 64);
        for x in 0..64 {
            w.paint(x, 50, 0, Material::Rock as u8); // floor at y=50
        }
        w.spawn_avatar(spawn_x, 5.0);
        for _ in 0..300 {
            w.step();
        }
        let [_, y, _, h] = w.avatar_xywh().unwrap();
        assert!(
            (y + h - 50.0).abs() <= 0.05,
            "spawn_x={spawn_x}: avatar's feet should rest essentially ON the floor surface (y+h≈50, got {})",
            y + h
        );
        let av = w.avatar_center().unwrap();
        assert!(av[1] < 50.0, "spawn_x={spawn_x}: avatar is above the floor, not through it");
    }
}

#[test]
fn avatar_is_blocked_by_a_wall() {
    // Loop over several fractional spawn offsets: the old buggy AABB only checked
    // cells up to floor(ax)+w-1, so whether the bug was caught depended on the
    // avatar's phase relative to the pixel grid (one offset could pass "by luck"
    // while another sank the avatar's right edge ~1px into the wall).
    for spawn_x in [30.0, 30.1, 30.2, 30.3, 30.5, 30.7, 30.9, 31.0, 31.3] {
        let mut w = World::new(64, 64);
        for x in 0..64 {
            w.paint(x, 50, 0, Material::Rock as u8); // floor
        }
        for y in 40..50 {
            w.paint(40, y, 0, Material::Rock as u8); // wall at x=40
        }
        w.spawn_avatar(spawn_x, 44.0);
        w.set_avatar_input(false, true, false); // walk right into the wall
        for _ in 0..300 {
            w.step();
        }
        let [x, _, aw, _] = w.avatar_xywh().unwrap();
        assert!(
            x + aw <= 40.05,
            "spawn_x={spawn_x}: avatar must not pass through the wall (right edge {} vs wall x=40)",
            x + aw
        );
    }
}

#[test]
fn avatar_does_not_poke_its_head_through_the_ceiling() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8); // floor
        w.paint(x, 40, 0, Material::Rock as u8); // ceiling, a couple cells above standing head
    }
    w.spawn_avatar(30.0, 44.0); // standing top edge at 44; ceiling's bottom face is at y=41
    for _ in 0..120 {
        w.step(); // settle onto the floor
    }
    w.set_avatar_input(false, false, true); // jump straight into the ceiling
    w.step();
    w.set_avatar_input(false, false, false);
    let mut min_y = 100.0f32;
    for _ in 0..60 {
        w.step();
        min_y = min_y.min(w.avatar_xywh().unwrap()[1]);
    }
    assert!(min_y >= 41.0 - 0.05, "avatar's head must not penetrate the ceiling (top edge {min_y} vs ceiling bottom=41)");
    // sanity: the jump was strong enough to actually reach the ceiling, so the bound
    // above is proving collision, not just an undershot jump.
    assert!(min_y < 44.0, "jump should have carried the avatar's head up near the ceiling, got {min_y}");
}

#[test]
fn avatar_falls_when_the_ground_is_carved_away() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 40, 0, Material::Rock as u8); // upper floor
        w.paint(x, 60, 0, Material::Rock as u8); // lower floor
    }
    w.spawn_avatar(30.0, 34.0);
    for _ in 0..120 {
        w.step();
    }
    let resting_y = w.avatar_xywh().unwrap()[1];
    // carve the floor out from under it
    for x in 25..40 {
        w.paint(x, 40, 0, Material::Empty as u8);
    }
    for _ in 0..300 {
        w.step();
    }
    let fallen_y = w.avatar_xywh().unwrap()[1];
    assert!(fallen_y > resting_y + 10.0, "avatar must fall after its ground is carved ({resting_y} -> {fallen_y})");
}

#[test]
fn avatar_can_jump_off_the_ground() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8);
    }
    w.spawn_avatar(30.0, 44.0);
    for _ in 0..120 {
        w.step(); // settle onto the floor
    }
    let grounded_y = w.avatar_xywh().unwrap()[1];
    w.set_avatar_input(false, false, true); // jump
    w.step();
    w.set_avatar_input(false, false, false);
    let mut min_y = grounded_y;
    for _ in 0..30 {
        w.step();
        min_y = min_y.min(w.avatar_xywh().unwrap()[1]);
    }
    assert!(min_y < grounded_y - 3.0, "jump must lift the avatar off the floor ({grounded_y} -> {min_y})");
}

#[test]
fn avatar_does_not_add_sim_work_when_resting() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8);
    }
    w.spawn_avatar(30.0, 44.0);
    for _ in 0..300 {
        w.step();
    }
    w.step();
    assert_eq!(w.cells_processed, 0, "a resting avatar writes no cells and keeps the world asleep");
}

#[test]
fn all_entity_kinds_settle_to_sleep() {
    // Guard the milestone's core invariant: with avatar, projectile, AND free particles all live
    // simultaneously, once everything settles and all transient entities are gone, the world
    // must return to cells_processed == 0 (fully asleep).
    let mut w = World::new(64, 64);

    // Build a solid floor so entities have something to land on
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8);
    }

    // Spawn an avatar above the floor
    w.spawn_avatar(30.0, 5.0);

    // Fire a projectile (kinetic ammo, non-zero velocity) toward the floor
    w.fire(10.0, 32.0, 6.0, 0.0, Ammo::Kinetic as u8);

    // Spawn a few free particles with small velocities
    w.spawn_particle(20.0, 10.0, 1.0, 0.0, Material::Sand as u8);
    w.spawn_particle(25.0, 15.0, -1.0, 0.0, Material::Sand as u8);
    w.spawn_particle(35.0, 20.0, 0.5, -0.5, Material::Sand as u8);

    // Step enough frames for everything to fully settle and all transient entities
    // (projectile impacts, particles) to die and the avatar to rest on the floor
    for _ in 0..600 {
        w.step();
    }

    // One more step to check that the world is asleep
    w.step();

    // After settling, the world must return to cells_processed == 0
    assert_eq!(w.cells_processed, 0, "with avatar, projectile, and particles all live and settled, world must be fully asleep");
}
