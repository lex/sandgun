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
  - **Fire** (material id 12, renders as FLAME) is **bright and warm** — red clearly beats blue.
  - **MushroomFlesh** (material id 7) glows **green-dominant** — green clearly beats both red and
    blue, a hue neither the grey depth-ambient nor the warm player light could produce.

  The far<near check alone can pass on depth-ambient contrast, so the hue checks are what prove the
  coloured-emission path specifically works.

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
