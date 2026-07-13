# SANDGUN — "Lighting & Atmosphere" Milestone (design, GRILLED & locked 2026-07-13)

The living fungal cavern, **lit from within**. Turns the flat full-bright renderer into a dark,
atmospheric cave lit by fire, bioluminescent fungi, and the player's own glow — the single
highest-impact step toward the Noita look. Worldgen shape is already in good shape; this is a
**rendering** milestone, sim untouched.

## Locked decisions (from grilling)

**Architecture — GPU, viewport-only, sim-decoupled.**
- Lighting is computed every frame in fragment shaders over just the visible **640×384 camera
  window** (not the whole 1024×2048 world), as a render-layer post-process.
- The **sim and chunk-sleep are completely untouched** — light is never written into cells or read
  by sim logic. This protects the sacred perf invariant: lighting cost is independent of world size.

**Light model — soft per-cell propagation (not per-light raymarch).**
- Build per-viewport **emission (RGB)** and **opacity** textures from cell material, then compute a
  lightmap by **iterative diffusion**: light spreads cell→cell through open space, multiplied down to
  zero at solid (opaque) cells. Scales to *thousands* of per-cell emitters for free (cost = passes,
  not light count).
- Shadows are therefore **soft** — light bends around corners and falls off — which reads beautifully
  for glowing fungi/fire in caves. (Not crisp hard-edged shadows; that would need discrete lights.)
- **Colored RGB** light. Computed at **half-resolution** (light is low-frequency) and upscaled, for
  performance.

**Ambient — dim, readable, depth-graded.** Underground has a low baseline visibility (you can make
out terrain shapes) with emitters/player-light adding drama on top — never fully blind. The **surface
is bright** (sky light) grading darker with depth; sky light pours into cave mouths.

**Emitters (colored, per-material emission table — tunable via params):**
- **Fire** — warm orange, bright, **flickering** (time-varying).
- **Bioluminescent fungi (the HERO visual)** — mushrooms + mycelium emit a soft **cyan-green** glow;
  the living cavern lit by its own life. Makes growth/colonies visually rewarding in the dark.
- **Spore gas** — green haze glow.
- **Acid** — sickly green glow.
- **Player light** — a soft warm glow following the avatar, guaranteeing baseline visibility near you.
- **Sky/surface** — bright ambient at/above the surface line.

**Occlusion:** solid terrain (Rock, Soil, MushroomFlesh) blocks light; Empty, gases (SporeGas,
Smoke), Fire, and thin Mycelium transmit it; liquids transmit (tinting deferred).

**Scope — lighting ONLY.** Deferred to later milestones: material palette rework, surface
moss/vegetation, normal-mapped shading, day/night, light-driven gameplay.

**Gameplay — purely visual this milestone.** No mechanics read light yet (that's a future milestone;
it would need CPU light readback, which the GPU-only architecture deliberately avoids for now).

**Kill criterion (grin test):** *Descend into a dark cavern and it's lit from within by a cluster of
bioluminescent mushrooms + your own soft light, with fire throwing flickering orange light and
shadows as it spreads — holding 60fps.* If that makes Lex grin, it shipped.

**Performance guardrail:** ≥60fps at the full 1024×2048 descent (lighting is viewport-scale, so
world size doesn't matter). Half-res lightmap + a bounded number of propagation passes. A debug key
toggles lighting on/off to compare.

## Technical integration (builds on the M1d renderer)

Current renderer: a persistent full-world RGBA texture (dirty-chunk `texSubImage` uploads), a
camera-window shader that draws the 640×384 viewport, and entities drawn as a JS 2D overlay pass.

New work:
1. **Material available per pixel.** The lightmap needs each cell's material (for emission + opacity).
   Pack the material id into the **alpha channel** of the existing world texture during `render()`
   (alpha is currently unused) — no extra upload, no new texture. Shader reads material from alpha →
   emission LUT + opacity.
2. **Multi-pass FBO pipeline** (WebGL2, new to the renderer, which is currently single-pass):
   - a. Extract emission (RGB) + opacity for the camera window (half-res target).
   - b. Inject dynamic lights not in the grid: **player light** (avatar position) and **sky light**
        (rows at/above the surface).
   - c. **Propagate** the lightmap via ping-pong FBOs: N diffusion passes, each spreading light to
        neighbours × per-cell transmission (0 at opaque). N sets the light radius.
   - d. **Composite**: `final = material_color × (ambient(depth) + lightmap)`, emitters added on top
        so they self-glow; tone/clamp.
   - e. **Entities** (avatar) sampled against the lightmap so they're lit by their surroundings.
   - f. **Flicker**: modulate fire emission by a cheap time-varying term.

## Risks / mitigations
- Propagation approximates occlusion (soft, not exact shadows) — accepted, looks good for caves.
- Half-res light bleed at edges — tune pass count / add a light upscale filter.
- Alpha-packing material must survive the dirty-chunk upload path — verify carried through.
- Perf: keep passes bounded + half-res; the debug toggle + fps HUD gate acceptance.

## Next step
`writing-plans` → subagent-driven execution. **Prerequisite:** merge the `worldgen-formations`
branch (organic worldgen + static soil) to master first, then branch `lighting` off master so it
builds on the finished terrain.
