use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn avatar_falls_and_rests_on_the_floor() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8); // floor at y=50
    }
    w.spawn_avatar(30.0, 5.0);
    for _ in 0..300 {
        w.step();
    }
    let [_, y, _, h] = w.avatar_xywh().unwrap();
    assert!((y + h - 50.0).abs() <= 1.5, "avatar's feet should rest on the floor (y+h≈50, got {})", y + h);
    let av = w.avatar_center().unwrap();
    assert!(av[1] < 50.0, "avatar is above the floor, not through it");
}

#[test]
fn avatar_is_blocked_by_a_wall() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8); // floor
    }
    for y in 40..50 {
        w.paint(40, y, 0, Material::Rock as u8); // wall at x=40
    }
    w.spawn_avatar(30.0, 44.0);
    w.set_avatar_input(false, true, false); // walk right into the wall
    for _ in 0..300 {
        w.step();
    }
    let [x, _, aw, _] = w.avatar_xywh().unwrap();
    assert!(x + aw <= 40.5, "avatar must not pass through the wall (right edge {} vs wall x=40)", x + aw);
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
