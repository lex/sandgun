use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn world_starts_empty() {
    let w = World::new(64, 64);
    assert_eq!(w.get(0, 0), Material::Empty);
    assert_eq!(w.get(63, 63), Material::Empty);
}

#[test]
fn paint_sets_a_single_cell_at_radius_zero() {
    let mut w = World::new(64, 64);
    w.paint(10, 10, 0, Material::Sand as u8);
    assert_eq!(w.get(10, 10), Material::Sand);
    assert_eq!(w.get(11, 10), Material::Empty);
}

#[test]
fn paint_circle_and_out_of_bounds_is_safe() {
    let mut w = World::new(64, 64);
    w.paint(0, 0, 5, Material::Water as u8); // clips off all four... two edges, must not panic
    assert_eq!(w.get(0, 0), Material::Water);
    assert_eq!(w.get(3, 0), Material::Water);
    assert_eq!(w.get(10, 10), Material::Empty);
}

#[test]
fn sand_falls_one_cell_per_step() {
    let mut w = World::new(64, 64);
    w.paint(10, 10, 0, Material::Sand as u8);
    w.step();
    assert_eq!(w.get(10, 10), Material::Empty);
    assert_eq!(w.get(10, 11), Material::Sand);
    w.step();
    assert_eq!(w.get(10, 12), Material::Sand);
}

#[test]
fn sand_settles_flat_on_the_floor() {
    // Classic loose-sand rule (user decision 2026-07-03): a grain blocked below
    // always tries the diagonals, so a lone 2-stack collapses flat on the floor.
    let mut w = World::new(64, 64);
    w.paint(10, 62, 0, Material::Sand as u8);
    w.paint(10, 60, 0, Material::Sand as u8);
    for _ in 0..10 {
        w.step();
    }
    assert_eq!(w.get(10, 63), Material::Sand, "first grain rests on the floor");
    assert!(
        w.get(9, 63) == Material::Sand || w.get(11, 63) == Material::Sand,
        "second grain topples to a floor neighbor"
    );
    assert_eq!(w.get(10, 62), Material::Empty, "no 2-stack survives on open floor");
}

#[test]
fn sand_slides_off_a_peak_diagonally() {
    let mut w = World::new(64, 64);
    // three stacked on the floor: the top one must topple left or right
    w.paint(32, 63, 0, Material::Sand as u8);
    w.paint(32, 62, 0, Material::Sand as u8);
    w.paint(32, 61, 0, Material::Sand as u8);
    for _ in 0..10 {
        w.step();
    }
    assert_eq!(w.get(32, 61), Material::Empty);
    assert!(
        w.get(31, 63) == Material::Sand || w.get(33, 63) == Material::Sand,
        "top grain should have toppled to a floor neighbor"
    );
}

#[test]
fn rock_never_moves() {
    let mut w = World::new(64, 64);
    w.paint(10, 10, 0, Material::Rock as u8);
    for _ in 0..5 {
        w.step();
    }
    assert_eq!(w.get(10, 10), Material::Rock);
    assert_eq!(w.get(10, 11), Material::Empty);
}

#[test]
fn pooled_water_comes_to_rest() {
    let mut w = World::new(64, 64);
    // rock basin: floor at y=60 spanning x=20..=30, walls up both sides
    for x in 20..=30 {
        w.paint(x, 60, 0, Material::Rock as u8);
    }
    for y in 50..60 {
        w.paint(20, y, 0, Material::Rock as u8);
        w.paint(30, y, 0, Material::Rock as u8);
    }
    w.paint(25, 52, 2, Material::Water as u8); // blob falls in and pools
    for _ in 0..400 {
        w.step();
    }
    w.step();
    assert_eq!(w.cells_processed, 0, "a pooled liquid must come to rest");
    assert_eq!(w.get(25, 59), Material::Water, "and it pooled at the basin floor");
}

#[test]
fn water_flows_toward_a_drop_and_off_a_ledge() {
    let mut w = World::new(64, 64);
    // rock shelf at y=40 spanning x=10..=20; open air to its right
    for x in 10..=20 {
        w.paint(x, 40, 0, Material::Rock as u8);
    }
    w.paint(18, 39, 0, Material::Water as u8); // 3 cells from the ledge at x=20
    for _ in 0..60 {
        w.step();
    }
    assert_eq!(w.get(18, 39), Material::Empty, "water found the drop and left the shelf");
    let below_shelf = (0..64).any(|x| (41..64).any(|y| w.get(x, y) == Material::Water));
    assert!(below_shelf, "it fell past the shelf");
}

#[test]
fn water_sinks_below_oil() {
    let mut w = World::new(64, 64);
    // rock walls stop the oil dispersing sideways before the swap is tested
    for y in 61..=63 {
        w.paint(9, y, 0, Material::Rock as u8);
        w.paint(11, y, 0, Material::Rock as u8);
    }
    w.paint(10, 63, 0, Material::Oil as u8);
    w.paint(10, 62, 0, Material::Water as u8);
    w.step();
    assert_eq!(w.get(10, 63), Material::Water, "denser water sinks");
    assert_eq!(w.get(10, 62), Material::Oil, "oil floats up");
}

#[test]
fn sand_sinks_through_water() {
    let mut w = World::new(64, 64);
    // walled column so the water can't slide out from under the sand
    for y in 61..=63 {
        w.paint(9, y, 0, Material::Rock as u8);
        w.paint(11, y, 0, Material::Rock as u8);
    }
    w.paint(10, 63, 0, Material::Water as u8);
    w.paint(10, 61, 0, Material::Sand as u8);
    for _ in 0..5 {
        w.step();
    }
    assert_eq!(w.get(10, 63), Material::Sand, "sand displaces the water");
    assert_eq!(w.get(10, 62), Material::Water, "water is pushed up, not deleted");
}

#[test]
fn liquids_do_not_pass_through_rock() {
    let mut w = World::new(64, 64);
    // rock cup: floor at y=50 spanning x=20..=24, walls at x=20 and x=24
    for x in 20..=24 {
        w.paint(x, 50, 0, Material::Rock as u8);
    }
    for y in 45..50 {
        w.paint(20, y, 0, Material::Rock as u8);
        w.paint(24, y, 0, Material::Rock as u8);
    }
    w.paint(22, 47, 0, Material::Water as u8);
    for _ in 0..30 {
        w.step();
    }
    let inside = (21..=23).any(|x| (45..=49).any(|y| w.get(x, y) == Material::Water));
    assert!(inside, "water must still be inside the rock cup");
}

#[test]
fn settled_world_processes_zero_cells() {
    let mut w = World::new(128, 128);
    w.paint(64, 100, 4, Material::Sand as u8);
    for _ in 0..300 {
        w.step(); // more than enough to fully settle
    }
    w.step();
    assert_eq!(w.cells_processed, 0, "settled chunks must be skipped entirely");
}

#[test]
fn painting_wakes_a_settled_world() {
    let mut w = World::new(128, 128);
    w.paint(64, 100, 4, Material::Sand as u8);
    for _ in 0..300 {
        w.step();
    }
    w.paint(64, 20, 2, Material::Sand as u8);
    w.step();
    assert!(w.cells_processed > 0, "paint must wake its chunk");
    for _ in 0..300 {
        w.step();
    }
    w.step();
    assert_eq!(w.cells_processed, 0, "and it must settle again");
}

#[test]
fn render_maps_material_palette_with_shade_jitter() {
    let mut w = World::new(64, 64);
    w.paint(5, 5, 0, Material::Sand as u8);
    w.render_rgba();
    let px = w.rgba();
    let o = (5 * 64 + 5) * 4;
    // Sand base [216, 184, 108], shade jitter is at most ±9 per channel
    assert!((px[o] as i16 - 216).abs() <= 9);
    assert!((px[o + 1] as i16 - 184).abs() <= 9);
    assert!((px[o + 2] as i16 - 108).abs() <= 9);
    // alpha carries the material id (lighting task 1)
    assert_eq!(px[o + 3], Material::Sand as u8);
    // empty cell renders the background color exactly, with alpha reporting Empty(0)
    let e = (0 * 64 + 0) * 4;
    assert_eq!(&px[e..e + 4], &[26, 24, 32, Material::Empty as u8]);
}

#[test]
fn displaced_liquid_rises_at_most_one_cell_per_frame() {
    // Packed column (walls to the floor): soil pillar directly on water, no empty space,
    // so the only possible motion is soil displacing water upward. Without stamping the
    // displaced liquid, a single water cell rides the bottom-up sweep cascade all the way
    // to the top of the pillar in one frame (teleport). Correct: it rises one cell per frame.
    let mut w = World::new(64, 64);
    for y in 47..64 {
        w.paint(9, y, 0, Material::Rock as u8);
        w.paint(11, y, 0, Material::Rock as u8);
    }
    for y in 54..64 {
        w.paint(10, y, 0, Material::Water as u8);
    }
    for y in 48..54 {
        w.paint(10, y, 0, Material::Soil as u8);
    }
    let top_water = |w: &World| (0..64).find(|&y| w.get(10, y) == Material::Water).unwrap();
    assert_eq!(top_water(&w), 54, "sanity: water starts at row 54");
    w.step();
    assert!(
        top_water(&w) >= 53,
        "displaced water must rise at most one cell per frame, not teleport (rose to row {})",
        top_water(&w)
    );
}

// --- M1d task 3: dirty-chunk GPU upload -------------------------------------------------

#[test]
fn render_dirty_starts_all_dirty_then_clears() {
    let mut w = World::new(128, 64); // 2x1 chunks
    assert!(w.render_dirty().iter().all(|&b| b == 1), "fresh world must upload once, in full");
    w.clear_render_dirty();
    assert!(w.render_dirty().iter().all(|&b| b == 0));
}

#[test]
fn render_dirty_set_on_mutation() {
    let mut w = World::new(128, 64); // chunk 0: x in [0,64); chunk 1: x in [64,128)
    w.clear_render_dirty();
    assert_eq!(w.render_dirty(), &[0, 0]);
    w.paint(10, 10, 0, Material::Sand as u8); // touches only chunk 0
    assert_eq!(w.render_dirty(), &[1, 0], "painting a cell must dirty only its own chunk");
}

#[test]
fn settled_world_has_no_render_dirty_after_clear() {
    let mut w = World::new(128, 64);
    w.clear_render_dirty();
    w.step(); // nothing active on a freshly-constructed, unwoken world
    assert!(
        w.render_dirty().iter().all(|&b| b == 0),
        "a settled world with nothing moving must not re-dirty any chunk"
    );
}

#[test]
fn render_rgba_only_touches_dirty_chunks() {
    let mut w = World::new(128, 64); // 2 chunks side by side
    w.render_rgba(); // first render: everything starts dirty
    w.clear_render_dirty();
    let before = w.rgba().to_vec();

    // Change chunk 0's material WITHOUT waking it (so render_dirty[chunk 0] stays 0) --
    // simulates "chunk 0 wasn't touched this frame" while giving render_rgba something wrong
    // to (incorrectly) pick up if it ignored the dirty bitmap.
    w.test_set_material_no_wake(10, 10, Material::Sand as u8);
    // Chunk 1 is mutated normally (paint wakes + dirties it).
    w.paint(70, 10, 0, Material::Water as u8);

    w.render_rgba();
    let after = w.rgba();

    let o0 = (10 * 128 + 10) * 4;
    assert_eq!(
        &after[o0..o0 + 4],
        &before[o0..o0 + 4],
        "render_rgba must not touch a chunk that isn't render-dirty"
    );

    let o1 = (10 * 128 + 70) * 4;
    assert!((after[o1] as i16 - 64).abs() <= 9, "dirty chunk 1 must reflect the new Water paint");
    assert!((after[o1 + 1] as i16 - 120).abs() <= 9);
    assert!((after[o1 + 2] as i16 - 220).abs() <= 9);
}
