# SANDGUN — Sharpened Plan

*Product of a /grill-me session, 2026-07-03. Supersedes the open questions in the original brief.*

## Vision (one sentence)

A browser-based falling-sand playground where guns are physics events and the fun is
**the world breaking apart** — chemistry (fire, acid, lava, cryo) is the trigger system,
physicality is the payoff.

## What this project is (and is not)

- **Tech-first playground.** The sim is the product. The roguelite layer (floors, unlocks,
  meta-progression, build-crafting) from the original brief is **parked indefinitely** — it gets
  its own grilling session only after the toy is proven fun.
- Built by Lex directing + Claude implementing. Milestones are defined by **observable,
  playtestable outcomes**, not code tasks. Dev-loop speed (hot-tweakable material params,
  debug paint tools) is a first-class requirement.

## Locked decisions

| Question | Decision |
|---|---|
| Goal | Tech-first playground; shipping is optional |
| Core fantasy | **Physicality first** — collapse, debris, craters; chemistry triggers it |
| Heat model | **Contact reactions only** (Noita-style pair table). No per-cell temperature field |
| World shape | Bounded scrolling floor, **vertical descent** — gravity and progression point the same way |
| Dev target | **Browser-first**: Rust → WASM, WebGL2 canvas. Native later, if ever |
| Perf bar | 60fps on Lex's Mac in-browser. No other hardware targets yet |
| Rigid bodies | **In scope** — but milestone 2, after the pixel toy is proven. Cell layout reserves for them from day one |
| Living things | Nothing alive until the toy grins. Enemies are a future grilling session |
| Milestone-1 kill criterion | **"Shooting sand is fun"** — 60 seconds of carving terrain and igniting oil must make you grin, or the concept fails cheap |

## Hero reaction families (implementation order)

Modest table, ~12–15 pairs total. Order:

1. **Fire chains** — spark → oil ignites → fire spreads → gas pocket explodes. The identity reaction; in milestone 1.
2. **Acid corrosion** — dissolves terrain, eats floors, drains lakes. In milestone 1 (it's the purest "physics-event bullet").
3. **Water / lava / steam** — lava + water → obsidian + steam. Terrain *creation*; milestone 1.5.
4. **Cryo freezing** — liquids → walkable/shatterable ice. The builder ammo; milestone 1.5.

## Technical decisions (engineering calls, made not grilled)

- **Sim**: single-buffer, bottom-up in-place update per the brief. Chunks (64×64) with dirty
  rects; settled chunks skipped. Single-threaded — no checkerboard multithreading until
  profiling on the Mac demands it (it likely won't at this world size).
- **World size**: parameterized; start ~1024 wide × 2048 tall (vertical descent). Camera is a
  simple follow-cam over a world texture; no streaming.
- **Cell layout**: 4 bytes/cell — `material: u8`, `shade/variant: u8`, `flags: u8`
  (settled, burning, **1 bit reserved for rigid-body ownership**), `aux: u8` (per-material
  scratch: fuel remaining, corrosion HP). No temperature byte — contact model doesn't need one.
- **Pixels-as-particles**: **required in milestone 1**, not polish. Cells can leave the grid as
  free-flying particles (velocity + gravity) and resettle — this is where crater ejecta,
  splashes, and debris feel comes from, and it carries the physicality fantasy until rigid
  bodies land in M2.
- **Projectiles**: entities above the grid (position, velocity, payload), collide against cells,
  apply a payload op on impact (inject material / displace radius / spawn particles).
- **Stack**: `wasm-bindgen` + `wasm-pack`, Vite dev server, WebGL2 textured quad blit of the
  material grid (palette lookup in fragment shader). No engine, no macroquad for now —
  browser-first means the web glue is the platform.
- **Rigid bodies (M2)**: Rapier2D (pure Rust, WASM-clean). Marching squares → simplified
  polygon → dynamic body → stamped back to grid each frame. Unsupported-terrain detection
  converts orphaned solid regions into bodies. This is the biggest tech risk in the project —
  hence sequenced after the toy is validated, and prototyped in a spike branch first.

## Milestones

### M0 — Skeleton (prove the pipeline)
Rust→WASM builds, canvas renders, mouse paints materials. Sand piles, water flows, oil floats
on water. Dirty-rect chunk skipping working (visible via debug overlay).
**Done when:** painting and watching materials is smooth at 60fps full-screen on the Mac.

### M1 — The Grin Test (kill criterion)
One gun, swappable ammo: kinetic (crater + ejecta particles), incendiary (fire chains), acid.
Materials: rock, sand, water, oil, flammable gas, wood, acid, fire, smoke. Scrolling
vertical world with follow-cam. Hot-reloadable material/reaction params.
**Done when:** 60 seconds of play makes Lex grin. If it doesn't, we diagnose whether it's
feel (fixable) or concept (stop cheap) before writing another line.

### M1.5 — Chemistry fill-out
Lava, water/lava→obsidian+steam, cryo ammo + ice. Round out the pair table to ~12–15.
**Done when:** every ammo type has a distinct terrain-verb, not just a damage color.

### M2 — The World Breaks (physicality payoff)
Rapier2D spike → integrate. Unsupported terrain falls as rigid chunks and shatters back to
pixels. Heavy ammo that severs support columns.
**Done when:** shoot a wooden support, watch the structure above crash down, grin again.

### M3+ — Parked (each needs its own grilling)
Enemies & damage model · roguelite loop (floors, guns-as-chassis, mods) · native/Steam build ·
multithreading · audio/art identity.

## Deliberately cut (this phase)

Per-cell temperature field · enemies/AI · meta-progression · native build · phone/laptop perf
targets · multithreading & SharedArrayBuffer headers · streaming world.
