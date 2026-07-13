# Lighting — headless check & tuning guide

The SANDGUN lighting pass is a **GPU, viewport-scale, sim-decoupled** pipeline: it recomputes a
lit image every frame over the visible 640×384 window (at half-res for the diffusion), reading
only the world colour/material texture. It never touches the simulation, so it is **chunk-sleep
safe** — a fully settled world (zero dirty chunks, sim asleep) still lights correctly because the
lightmap is derived from the GPU world texture each frame, not from sim activity.

Pipeline per frame (`web/src/main.js` frame loop → `web/src/renderer.js`):
1. `seedEmission(ctx, camX, camY, time)` — paint each cell's emission colour into the half-res
   lightmap (`emissionFor`), with a subtle time-varying **fire flicker** applied to FLAME only.
2. `propagate(ctx, camX, camY, LIGHT_PASSES)` — diffuse the emission for `LIGHT_PASSES` passes,
   blocked by opaque terrain (soft occlusion).
3. `drawLit(...)` — composite: `worldColour × (depthAmbient + diffusedLight + playerLight)`.

## Running the headless check

The check drives the real page with Playwright, so the dev server must be up:

```bash
cd web && npm run dev            # serves http://localhost:5173 (wasm must be built into web/src/pkg)
node web/tests-lighting/check.mjs   # from the repo root; prints sampled values, ends with OK / FAIL
```

Exit code is non-zero on any failed assertion. It uses a fixed seed (`generate(7)`) for a
deterministic scene.

### What it asserts

- **No GL errors / no console errors** during boot + render.
- **Depth-ambient contrast**: a deep/far sample is darker than a sample near the avatar
  (`farLum < nearLum`, and `farLum` is genuinely dark).
- **Emission actually contributes (hue-specific)**: it paints guaranteed emitters at fixed world
  cells ~160 px from the avatar (outside the 90 px warm player light so that light can't fake the
  result), repaints every frame, and samples them through the current camera:
  - **Fire** (material id 12, renders as FLAME) is checked for brightness on its own cell, but that
    alone is **not** proof of emission — Fire's base world colour is already orange
    ([255,150-220,40]) regardless of lighting, so a warm reading there could just be plain material
    colour. The check that actually proves fire's light propagates samples a patch of guaranteed
    **open, EMPTY air one cell beside the fire's radius** (with a forced-open corridor so terrain
    can't block it), and compares it against a same-depth, guaranteed-unlit Empty baseline
    elsewhere. That comparison — not an absolute reading — is required because Empty's base colour
    ([26,24,32]) isn't perfectly neutral: it leans slightly blue (B > R), which partly cancels
    fire's warm tint in the multiplicative composite, so `gapNear.r > gapNear.b` alone is too weak
    a signal (direct instrumentation of the raw lightmap confirms the light genuinely arrives there
    with a clear warm skew — the composited-canvas cancellation is a rendering-math artifact, not
    a sign the emission isn't propagating). Comparing the near sample's warmth/brightness *delta*
    against the far baseline isolates fire's contribution and was verified stable (R-B delta 3-4,
    luminance delta ~3) across many runs. The far baseline itself is placed on whichever screen
    edge is farther from the avatar's actual position, since a fixed mid-screen location can drift
    into the avatar's own 90 px player light and quietly corrupt the "unlit" control.
  - **MushroomFlesh** (material id 7) glows **green-dominant** on its neutral beige base material
    ([232,208,186]) — green clearly beats both red and blue, a hue neither the grey depth-ambient
    nor the warm player light could produce. No delta-vs-baseline trick is needed here since beige
    can never itself read as green.

  The far<near check alone can pass on depth-ambient contrast, so the fungi green-dominance and the
  fire adjacent-open-air warmth delta are what prove the coloured-emission/propagation path
  specifically works (fire's own-cell reading doesn't discriminate, since it's intrinsically orange
  already).

## In-game toggle

Press **`L`** to toggle lighting on/off (`input.lightingOn`). Off falls back to the flat
`drawCamera` render — handy for A/B comparison of the lit vs unlit scene.

## Tuning knobs (where they live)

| Knob | Location | Effect |
| --- | --- | --- |
| `LIGHT_PASSES` | `web/src/main.js` (near `VIEW_W`/`VIEW_H`) | Diffusion passes/frame = **light reach**. Higher → farther/softer light (costlier); lower → tighter/cheaper. |
| `u_falloff` | `propagate()` in `web/src/renderer.js` (`gl.uniform1f(light.propFalloffLoc, …)`) | Per-pass retention = how far light **travels**. Higher → travels farther. |
| Depth-ambient endpoints | `COMP_FS` in `web/src/renderer.js` (`mix(vec3(0.75…), vec3(0.06…), depth)`) | Surface vs deep ambient brightness — keep caves moody but readable. |
| Player-light radius / colour / intensity | `COMP_FS` (`u_playerR`, the `vec3(1.0,0.85,0.6)…` term) + radius passed from `drawLit` call in `main.js` | The avatar's warm personal glow. |
| Emitter colours / intensities | `emissionFor()` — **duplicated in `SEED_FS` and `PROP_FS`** | Per-material light colour (MushroomFlesh, Mycelium, SporeGas, Acid, FLAME). |
| Fire flicker | `SEED_FS` main in `web/src/renderer.js` (the `0.82 + 0.18*sin(…)` term) | Subtle spatial+temporal shimmer on FLAME; driven by `u_time` (`performance.now()/1000` from `main.js`). Seed stage only. |

> **`emissionFor` is duplicated** in `SEED_FS` and `PROP_FS` (no GLSL include without a build step).
> The two bodies must stay **byte-identical** — edit both together. The fire flicker is applied
> separately in `SEED_FS`'s `main` (not inside `emissionFor`), so the two bodies stay identical.

> **Lightmap sampling:** the diffused lightmap is **window-scale** (its 0..1 uv spans the visible
> view), not the world texture. The composite samples it with the screen-normalized coordinate
> (`gl_FragCoord.xy / u_viewSize`), not the world-space `v_uv` — sampling with `v_uv` reads the
> wrong cell and the emission never lands on screen.
