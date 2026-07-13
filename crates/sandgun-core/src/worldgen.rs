use crate::cell::{Cell, Material};
use crate::world::World;

/// Generation-local xorshift32 so the sim's own RNG stream is untouched.
struct GenRng(u32);

impl GenRng {
    fn next(&mut self) -> u32 {
        let mut r = self.0;
        r ^= r << 13;
        r ^= r >> 17;
        r ^= r << 5;
        self.0 = r;
        r
    }
    /// true with probability num/den
    fn chance(&mut self, num: u32, den: u32) -> bool {
        self.next() % den < num
    }
    /// uniform in [lo, hi)
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo) as u32) as i32
    }
}

fn set(world: &mut World, x: usize, y: usize, m: Material, rng: &mut GenRng) {
    let i = y * world.width + x;
    world.cells[i] = Cell::new(m, (rng.next() & 3) as u8);
}

/// A depth band of the subsurface: a soil/rock mix ratio (before caves) plus the initial cave
/// open-cell density the Task 2 CA carve will use for that depth.
struct Biome {
    top: usize,    // inclusive world row where this band starts
    soil_pct: u32, // 0..100 chance a filled cell is Soil vs Rock (before caves)
    // 0..100 initial open-cell chance for the cave CA carve (pass 3), per depth band.
    cave_seed_pct: u32,
}

/// Partition [surface .. h) into depth bands: a soil-rich crust zone near the top grading to
/// rock-dominant depths. Boundaries scale with world height so it works at any size.
fn biomes(h: usize) -> Vec<Biome> {
    vec![
        Biome { top: 0, soil_pct: 78, cave_seed_pct: 42 },        // upper: soft, soil-rich
        Biome { top: h * 2 / 5, soil_pct: 45, cave_seed_pct: 46 }, // mid: mixed
        Biome { top: h * 7 / 10, soil_pct: 20, cave_seed_pct: 40 }, // deep: rock-dominant
    ]
}

fn biome_at<'a>(bands: &'a [Biome], y: usize) -> &'a Biome {
    bands.iter().rev().find(|b| y >= b.top).unwrap_or(&bands[0])
}

fn blob(world: &mut World, rng: &mut GenRng, cx: i32, cy: i32, radius: i32, m: Material) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x as usize >= world.width || y as usize >= world.height {
                continue;
            }
            set(world, x as usize, y as usize, m, rng);
        }
    }
}

/// BFS-flood Empty cells from the open sky (top row) using 4-connectivity. Returns whether the
/// bottom row was reached, plus the deepest (max-y) open cell reached (first one found at that
/// depth, if there are ties) -- the launch point for `ensure_descendable`'s connectivity carve.
fn flood_from_surface(world: &World) -> (bool, usize, usize) {
    use std::collections::VecDeque;
    let (w, h) = (world.width, world.height);
    let mut seen = vec![false; w * h];
    let mut q = VecDeque::new();
    for x in 0..w {
        if world.get(x, 0) == Material::Empty {
            seen[x] = true;
            q.push_back((x, 0usize));
        }
    }
    let mut reached_bottom = false;
    let (mut deepest_x, mut deepest_y) = (0usize, 0usize);
    while let Some((x, y)) = q.pop_front() {
        if y > deepest_y {
            deepest_y = y;
            deepest_x = x;
        }
        if y == h - 1 {
            reached_bottom = true;
        }
        for (nx, ny) in [
            (x as i32 - 1, y as i32),
            (x as i32 + 1, y as i32),
            (x as i32, y as i32 - 1),
            (x as i32, y as i32 + 1),
        ] {
            if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            if !seen[ny * w + nx] && world.get(nx, ny) == Material::Empty {
                seen[ny * w + nx] = true;
                q.push_back((nx, ny));
            }
        }
    }
    (reached_bottom, deepest_x, deepest_y)
}

/// Guarantee a top-to-bottom Empty path exists. The cavern CA carve above is density-driven and
/// can seal pockets off from each other (especially in the rock-dominant deep band), so after
/// carving we flood Empty from the open sky; if the bottom row wasn't reached, carve a
/// meandering shaft -- a downward-biased random walk, x jittered by rng.range(-1,2) each step,
/// width 2-3 cells -- from the deepest surface-reachable point down to the bottom row. This
/// guarantees a descendable path regardless of how the CA carve happened to connect; caves then
/// read as branches off this guaranteed route.
fn ensure_descendable(world: &mut World, rng: &mut GenRng) {
    let (w, h) = (world.width, world.height);
    let (reached_bottom, x0, y0) = flood_from_surface(world);
    if reached_bottom {
        return;
    }
    // Random walk downward from the deepest surface-reachable open cell. Each row's carved span
    // always covers BOTH the previous and the new center column (plus a 1-cell margin), so
    // consecutive rows are guaranteed to share at least one column -- that's what makes the shaft
    // provably 4-connected end to end, regardless of how the jitter happens to land.
    let mut cur_x = x0 as i32;
    let mut y = y0;
    while y < h - 1 {
        y += 1;
        let next_x = (cur_x + rng.range(-1, 2)).clamp(0, w as i32 - 1);
        let lo = (cur_x.min(next_x) - 1).clamp(0, w as i32 - 1);
        let hi = (cur_x.max(next_x) + 1).clamp(0, w as i32 - 1);
        for cx in lo..=hi {
            set(world, cx as usize, y, Material::Empty, rng);
        }
        cur_x = next_x;
    }
}

/// Solid terrain a set-piece can rest on or be carved into (never air/liquid/gas/organic).
fn is_terrain(world: &World, x: usize, y: usize) -> bool {
    matches!(world.get(x, y), Material::Rock | Material::Soil)
}

/// Whether (x, y) is a STABLE floor/wall support -- solid terrain that will not move on the first
/// sim step. Rock is immovable, so it is always support. Soil is a POWDER (it falls), so a Soil
/// cell only counts as support when it is itself held up: the cell directly below is non-Empty (the
/// world floor at the bottom edge counts). A lone Soil cell sitting over Empty would fall on frame
/// one, so it is NOT support. Checking one row down is enough: in a Soil-on-Soil-on-Rock column
/// every cell is supported, while a floating Soil cell is caught here. This is the right predicate
/// for "can a poured pool / stamped bed rest here?" -- `is_terrain` (which is true for unsupported
/// Soil) is not.
fn is_support(world: &World, x: usize, y: usize) -> bool {
    match world.get(x, y) {
        Material::Rock => true,
        Material::Soil => y + 1 >= world.height || world.get(x, y + 1) != Material::Empty,
        _ => false,
    }
}

/// Height (in cells) of the solid wall at column `x` counting upward from row `y` inclusive --
/// how many consecutive STABLE-support cells stack from (x, y) toward the surface. Used to cap how
/// deep a liquid pool can be poured against a bounding wall so it can never spill over the top. Uses
/// `is_support` (not `is_terrain`) so an unsupported Soil cell -- which would fall and let the pool
/// spill -- doesn't count toward the wall.
fn wall_height(world: &World, x: usize, y: usize) -> i32 {
    let mut n = 0;
    let mut yy = y as i32;
    while yy >= 0 && is_support(world, x, yy as usize) {
        n += 1;
        yy -= 1;
    }
    n
}

/// Task 3 set-piece: stamp fertile Soil beds onto cavern FLOORS (an Empty cell with solid terrain
/// directly below). Biome-weighted via `soil_pct` so beds are common in the soft upper band and
/// rare deep. Each bed converts a small patch of the floor terrain to Soil; the soil cell just
/// under the open floor is returned as a preferred colony-origin site -- a colony germinates there
/// on food and grows up into the open cavern. Only ever overwrites terrain (never air/liquid).
fn place_soil_beds(
    world: &mut World,
    rng: &mut GenRng,
    surface: &[i32],
    bands: &[Biome],
) -> Vec<(usize, usize)> {
    let (w, h) = (world.width, world.height);
    let mut beds: Vec<(usize, usize)> = Vec::new();
    let target = (w / 8).max(8);
    let mut attempts = 0;
    while beds.len() < target && attempts < 8000 {
        attempts += 1;
        let x = rng.range(2, w as i32 - 2) as usize;
        let lo = surface[x] + 5;
        if lo >= h as i32 - 3 {
            continue;
        }
        let y = rng.range(lo, h as i32 - 3) as usize;
        // must be a cavern floor: open cell sitting directly on STABLE support (not a Soil cell
        // that is itself floating over Empty, which would fall and drop the bed).
        if world.get(x, y) != Material::Empty || !is_support(world, x, y + 1) {
            continue;
        }
        // biome weighting: reuse soil_pct as a fertility weight (upper 78% vs deep 20%)
        if !rng.chance(biome_at(bands, y).soil_pct, 100) {
            continue;
        }
        // stamp a small soil patch into the floor terrain below/beside the site
        for dx in -1i32..=1 {
            for dy in 1i32..=2 {
                let (bx, by) = (x as i32 + dx, y as i32 + dy);
                if bx < 0 || by < 0 || bx as usize >= w || by as usize >= h {
                    continue;
                }
                let (bx, by) = (bx as usize, by as usize);
                // Only overwrite terrain (never air/liquid) AND only where the new Soil cell will
                // itself be supported (non-Empty directly below, or the world floor) -- converting a
                // supported Rock cell that has Empty under it into Soil would just recreate the
                // float bug we're fixing. Converting terrain to Soil never creates Empty, so the
                // current below-cell state is the final state.
                let supported_below = by + 1 >= h || world.get(bx, by + 1) != Material::Empty;
                if is_terrain(world, bx, by) && supported_below {
                    set(world, bx, by, Material::Soil, rng);
                }
            }
        }
        beds.push((x, y + 1)); // fertile top-of-bed cell, exposed to cave air
    }
    beds
}

/// Task 3 set-piece: pour shallow liquid pools into cavern BASINS. A basin is a run of >=3 open
/// cells that all sit directly on solid terrain (a flat floor) and is bounded by solid walls on
/// BOTH ends, so poured liquid comes to rest instead of a floating blob. Pool depth is capped by
/// the bounding walls' height (and each added row must be fully open across the run) so it can
/// never spill. Liquid type is depth-graded: water shallow, oil mid, acid deep and rare. Pools are
/// kept shallow; the descendability guarantee is re-run after this so a pool can't seal the descent.
fn place_liquid_pools(world: &mut World, rng: &mut GenRng) {
    let (w, h) = (world.width, world.height);
    // Per-zone caps (not one global cap): scanning is top-down, so a single cap would be spent on
    // the shallow water zone before the scan ever reached the oil/acid zones. Separate budgets
    // guarantee each depth band gets its pools regardless of scan order.
    let (oil_top, acid_top) = (h / 2, h * 4 / 5);
    let (water_cap, oil_cap, acid_cap) = ((w / 16).max(6), (w / 16).max(6), (w / 40).max(2));
    let (mut water, mut oil, mut acid) = (0usize, 0usize, 0usize);
    for y in 1..h - 1 {
        if water >= water_cap && oil >= oil_cap && acid >= acid_cap {
            break;
        }
        let mut x = 1;
        while x < w - 1 {
            // find the start of a flat floor run. The floor must be STABLE support (not a Soil cell
            // floating over Empty), or the poured liquid wouldn't actually be "at rest" -- the floor
            // would fall out from under it on the first sim step.
            if world.get(x, y) != Material::Empty || !is_support(world, x, y + 1) {
                x += 1;
                continue;
            }
            let start = x;
            while x < w - 1 && world.get(x, y) == Material::Empty && is_support(world, x, y + 1) {
                x += 1;
            }
            let end = x - 1; // inclusive; x now sits just past the run
            // require a genuine basin: >=3 wide and walled on both ends by stable support at this row
            if end - start + 1 < 3 || !is_support(world, start - 1, y) || !is_support(world, end + 1, y) {
                continue;
            }
            // depth-graded liquid: acid is deep AND rare, oil mid, water shallow. Skip a zone once
            // its budget is full (acid also rolls a rarity chance so deep pools are mostly dry).
            let mat = if y >= acid_top {
                if acid >= acid_cap || !rng.chance(1, 3) {
                    continue;
                }
                acid += 1;
                Material::Acid
            } else if y >= oil_top {
                if oil >= oil_cap {
                    continue;
                }
                oil += 1;
                Material::Oil
            } else {
                if water >= water_cap {
                    continue;
                }
                water += 1;
                Material::Water
            };
            // deepen only while both walls out-rise the fill and the next row up is fully open
            let mut depth = 1usize;
            while depth < 3
                && y >= depth
                && wall_height(world, start - 1, y) as usize > depth
                && wall_height(world, end + 1, y) as usize > depth
                && (start..=end).all(|cx| world.get(cx, y - depth) == Material::Empty)
            {
                depth += 1;
            }
            for cy in (y + 1 - depth)..=y {
                for cx in start..=end {
                    if world.get(cx, cy) == Material::Empty {
                        set(world, cx, cy, mat, rng);
                    }
                }
            }
        }
    }
}

/// Task 3 set-piece: light material pockets kept from the old scatter -- sand dunes straddling the
/// surface (they slump alive on frame one) and spore-gas blobs floating in cave air. Lighter than
/// before; the soil beds and basin pools are the main set-pieces now.
fn place_pockets(world: &mut World, rng: &mut GenRng, surface: &[i32]) {
    let (w, h) = (world.width, world.height);
    // sand dunes straddling the surface
    for _ in 0..(w / 24).max(4) {
        let x = rng.range(6, w as i32 - 6);
        let sy = surface[x as usize] - 2;
        let r = rng.range(3, 7);
        blob(world, rng, x, sy, r, Material::Sand);
    }
    // spore-gas blobs in cave air; they rise and pool by themselves
    let mut placed = 0;
    let target = (w / 20).max(5);
    let mut attempts = 0;
    while placed < target && attempts < 3000 {
        attempts += 1;
        let x = rng.range(4, w as i32 - 4);
        let lo = surface[x as usize] + 4;
        if lo >= h as i32 - 4 {
            continue;
        }
        let y = rng.range(lo, h as i32 - 4);
        if world.get(x as usize, y as usize) != Material::Empty {
            continue;
        }
        let r = rng.range(2, 4);
        blob(world, rng, x, y, r, Material::SporeGas);
        placed += 1;
    }
}

pub fn generate(world: &mut World, seed: u32) {
    let (w, h) = (world.width, world.height);
    let mut rng = GenRng(seed.wrapping_mul(0x9E37_79B9) | 1);
    world.clear();

    // 1. surface heightline: bounded random walk, box-smoothed
    let mut surface = vec![0i32; w];
    let mut y = h as i32 / 3;
    for x in 0..w {
        y += rng.range(-1, 2);
        y = y.clamp(h as i32 / 5, h as i32 / 2);
        surface[x] = y;
    }
    for _ in 0..2 {
        let prev = surface.clone();
        for x in 1..w - 1 {
            surface[x] = (prev[x - 1] + prev[x] + prev[x + 1]) / 3;
        }
    }

    // 2. depth-graded base fill: soil-rich near the surface grading to rock-dominant deep, via
    // per-band per-cell rolls (replaces the old uniform-rock-then-global-soil-CA passes). Caves
    // are carved into this fill below (pass 3); soil beds/liquid pools are set-pieces for Task 3.
    let bands = biomes(h);
    let mut soil_mask = vec![false; w * h];
    for x in 0..w {
        for yy in surface[x] as usize..h {
            let band = biome_at(&bands, yy);
            soil_mask[yy * w + x] = rng.chance(band.soil_pct, 100);
        }
    }
    // 2b. majority-smoothing: 2 passes of the standard "majority of 8 neighbors wins, ties keep
    // the current state" rule kill salt-and-pepper noise so each band reads as a cohesive soil or
    // rock mass rather than a checkerboard. Only touches cells at/below the surface line so the
    // open sky above is untouched.
    for _ in 0..2 {
        let prev = soil_mask.clone();
        for x in 1..w - 1 {
            for yy in 1..h - 1 {
                if (yy as i32) < surface[x] {
                    continue;
                }
                let mut n = 0;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if (dx, dy) == (0, 0) {
                            continue;
                        }
                        if prev[((yy as i32 + dy) as usize) * w + (x as i32 + dx) as usize] {
                            n += 1;
                        }
                    }
                }
                soil_mask[yy * w + x] = if n == 4 { prev[yy * w + x] } else { n > 4 };
            }
        }
    }
    for x in 0..w {
        for yy in surface[x] as usize..h {
            let m = if soil_mask[yy * w + x] { Material::Soil } else { Material::Rock };
            set(world, x, yy, m, &mut rng);
        }
    }

    // 3. caves: cellular automata in the rock body, sparing a 4-cell crust. The initial open-cell
    // density is per-biome (cave_seed_pct) rather than a flat rate, so caverns are sparser in the
    // soft upper band and looser in the mixed/deep bands -- the CA smoothing (isotropic 8-neighbor
    // majority, below) then reads as organic pockets rather than streaks regardless of band.
    let mut open = vec![false; w * h];
    for x in 0..w {
        for yy in 0..h {
            if (yy as i32) > surface[x] + 4 {
                let band = biome_at(&bands, yy);
                open[yy * w + x] = rng.chance(band.cave_seed_pct, 100);
            }
        }
    }
    for _ in 0..4 {
        let prev = open.clone();
        for x in 1..w - 1 {
            for yy in 1..h - 1 {
                if (yy as i32) <= surface[x] + 4 {
                    continue;
                }
                let mut n = 0;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if (dx, dy) == (0, 0) {
                            continue;
                        }
                        if prev[((yy as i32 + dy) as usize) * w + (x as i32 + dx) as usize] {
                            n += 1;
                        }
                    }
                }
                open[yy * w + x] = if prev[yy * w + x] { n >= 4 } else { n >= 5 };
            }
        }
    }
    for x in 0..w {
        for yy in 0..h {
            if open[yy * w + x] {
                set(world, x, yy, Material::Empty, &mut rng);
            }
        }
    }

    // 3d. structured set-pieces (Task 3): replaces the old blob-scatter (spore/sand/water/oil
    // blobs). Soil beds are stamped on cavern floors first (fertile germination sites for
    // colonies), then shallow liquid pools are poured into genuine basins (bounded, so they
    // settle rather than churn or float), then light sand/spore pockets. Every pass here writes
    // solid/liquid/gas material and so must run BEFORE the descendability guarantee below.
    let bed_sites = place_soil_beds(world, &mut rng, &surface, &bands);
    place_liquid_pools(world, &mut rng);
    place_pockets(world, &mut rng, &surface);

    // 4. guaranteed descendable path: the cavern carve is a density-driven CA and is not
    // guaranteed to connect the surface all the way to the bottom of the world (pockets can seal
    // off from each other), and the set-pieces above can fill pieces of a carved cave with
    // non-Empty material (a liquid pool could even seal the main descent). So this check runs
    // last, after every pass that can still write solid/liquid/gas material, right before the
    // world goes live: flood-fill Empty from the open sky and, if the bottom row wasn't reached,
    // carve a meandering shaft down from the deepest surface-reachable point so the world is
    // always descendable top-to-bottom. Caves (and any pool the shaft drains through) then read as
    // branches off this guaranteed route. Nothing after this point writes Empty, so the descent
    // stays open.
    ensure_descendable(world, &mut rng);

    // 5. everything settles alive
    world.wake_all();

    // 6. bake substrate richness into every Soil cell's aux so mycelium tips have something to
    // eat as soon as colonies start growing.
    let richness_min = world.params.values[crate::params::P_SOIL_RICHNESS_MIN] as i32;
    let richness_max = world.params.values[crate::params::P_SOIL_RICHNESS_MAX] as i32;
    let mut soil_sites: Vec<(usize, usize)> = Vec::new();
    for x in 0..w {
        for yy in 0..h {
            if world.get(x, yy) == Material::Soil {
                let richness = rng.range(richness_min, richness_max + 1).clamp(0, 255) as u8;
                world.set_soil_richness(x, yy, richness);
                soil_sites.push((x, yy));
            }
        }
    }

    // 7. seed colony origins: each colony starts as one mycelium cell + one tip on a Soil cell,
    // so it has substrate to eat immediately. Replaces the old pre-filled mycelium veins/mushroom
    // groves -- the world now grows its own mycelium (and, eventually, mushrooms) outward from
    // these origins via the living organism model, rather than starting pre-grown. Task 3: bias
    // origins onto the fertile soil beds stamped on cavern floors (they're Soil exposed to open
    // cave air, so a colony germinates on food and can grow up into the cavern) -- fall back to any
    // soil site if no beds were placed.
    let colony_count = world.params.values[crate::params::P_MY_WORLDGEN_COLONIES] as usize;
    let origins: &[(usize, usize)] = if !bed_sites.is_empty() { &bed_sites } else { &soil_sites };
    if !origins.is_empty() {
        for _ in 0..colony_count {
            let (sx, sy) = origins[(rng.next() as usize) % origins.len()];
            world.spawn_colony(sx, sy);
        }
    }
}
