# Lighting & Atmosphere Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn SANDGUN's flat full-bright renderer into a dark, atmospheric cave lit by fire, bioluminescent fungi, and the player's own glow — a GPU, viewport-only lighting layer that never touches the sim or chunk-sleep.

**Architecture:** A multi-pass WebGL2 pipeline added to `web/src/renderer.js`. The sim keeps writing the persistent full-world RGBA texture (M1d). We pack each cell's *material id* into that texture's alpha channel (Task 1), then each frame: (a) seed a half-resolution emission+opacity lightmap for the visible camera window from material, (b) diffuse light cell-to-cell blocked by solid terrain (ping-pong FBOs — soft occlusion shadows, scales to any emitter count), (c) inject a player light, (d) composite `worldColor × (depthAmbient + light)` to screen. Colored RGB. Sim untouched — light is never read back.

**Tech Stack:** Rust (`sandgun-core`, the one core change), WebGL2 GLSL ES 3.00 (`web/src/renderer.js`), plain JS (`web/src/main.js`). No new dependencies. Headless verification via Playwright from the global npx cache (`/Users/lex/.npm/_npx/e41f203b7505f1fb/node_modules/playwright/index.js`, CJS default import) + `gl.readPixels` assertions + screenshots.

## Global Constraints

- **Chunk-sleep is sacred.** Lighting is a pure render-layer post-process over the 640×384 viewport. It must NOT wake chunks, write cells, change `cells_processed`, or make the sim non-deterministic. The only sim-crate change allowed is packing material-id into the render RGBA alpha (Task 1), which is cosmetic output only.
- **Sim determinism unaffected:** Task 1 must not touch `next_rand` usage or cell state — only the `rgba` output bytes. Confirm `cargo test -p sandgun-core` stays green (no RNG-stream shift).
- **Viewport-scale cost only:** all lighting work is sized to the 640×384 window (lightmap at half = 320×192), independent of the 1024×2048 world. Target ≥60fps.
- **WebGL2** (already required by `initGL`). Half-float render targets need `EXT_color_buffer_float` (query it; fall back to `RGBA8` targets if absent — light values are 0..~4, an 8-bit target with a fixed /4 scale is acceptable; prefer float when available).
- **Wasm rebuild:** after any `sandgun-core` change, run `./scripts/build-wasm.sh` before the browser will see it.
- Commits end with a second `-m` line: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- **Branch:** `lighting` (already created off master).

## Material id / alpha encoding (used by Tasks 1–3)

Material ids (from `crates/sandgun-core/src/cell.rs`): Empty=0 Rock=1 Sand=2 Water=3 Oil=4 Soil=5 Mycelium=6 MushroomFlesh=7 SporeGas=8 Smoke=9 Ash=10 Acid=11 Fire=12. Plus a synthetic **FLAME=13** the renderer writes for any cell that is on fire (burning flag) or is Fire, so the light shader treats all flame identically (orange emitter, transmissive).

- **Opaque (blocks light):** Rock(1), Sand(2), Soil(5), MushroomFlesh(7), Ash(10).
- **Transmissive (light passes):** Empty(0), Water(3), Oil(4), Mycelium(6), SporeGas(8), Smoke(9), Acid(11), Fire(12), FLAME(13).
- **Emitters (color, pre-intensity):** Mycelium(6)=(0.10,0.55,0.45) dim; MushroomFlesh(7)=(0.25,1.0,0.65) bright (hero); SporeGas(8)=(0.30,0.85,0.20); Acid(11)=(0.45,1.0,0.10); FLAME(13)=(1.0,0.62,0.18). All others = 0.

## File structure

- `crates/sandgun-core/src/world.rs` — modify `render_rgba` (~line 1132) to write material-id/FLAME into `rgba[o+3]` instead of `255`.
- `crates/sandgun-core/tests/render.rs` — **create**: Rust unit tests for the alpha encoding.
- `web/src/renderer.js` — the bulk of the work: new shaders (emission seed, propagation, composite), FBO/texture setup, and exported functions `computeLighting(ctx, camX, camY, opts)` + `drawLit(ctx, camX, camY, opts)`; keep `initGL`/`uploadDirtyChunks` and add lighting resources inside `initGL`'s returned ctx.
- `web/src/main.js` — swap `drawCamera(...)` for the lighting path; pass avatar position + a `lightingOn` toggle; add a debug key.
- `web/src/input.js` — add an `L` key toggle for lighting (read the current key-handling pattern there first).
- `web/tests-lighting/check.mjs` — **create**: a committed headless verification script (documented, run manually / in the future e2e job).

---

### Task 1: Pack material id (and a FLAME code) into the render RGBA alpha

**Files:**
- Modify: `crates/sandgun-core/src/world.rs` (`render_rgba`, ~1132–1162)
- Create: `crates/sandgun-core/tests/render.rs`

**Interfaces:**
- Produces: after `render_rgba()`, for every cell `rgba[i*4+3]` = `13` if the cell is burning or Fire, else the cell's material id (`0..=12`). RGB channels unchanged. The web renderer (Tasks 2–3) reads this alpha as material.

- [ ] **Step 1: Write the failing test** (`crates/sandgun-core/tests/render.rs`):

```rust
use sandgun_core::cell::Material;
use sandgun_core::world::World;

// Alpha channel of the rendered RGBA carries the material id so the web lighting shader knows each
// cell's emission/opacity. Burning cells (and Fire) report a synthetic FLAME code = 13.
const FLAME: u8 = 13;

fn alpha_at(w: &World, x: usize, y: usize) -> u8 {
    let rgba = w.rgba();
    rgba[(y * w.width + x) * 4 + 3]
}

#[test]
fn alpha_encodes_material_id() {
    let mut w = World::new(64, 64);
    w.paint(10, 10, 0, Material::Rock as u8);
    w.paint(12, 10, 0, Material::Soil as u8);
    w.paint(14, 10, 0, Material::MushroomFlesh as u8);
    w.mark_all_render_dirty();
    w.render_rgba();
    assert_eq!(alpha_at(&w, 10, 10), Material::Rock as u8);
    assert_eq!(alpha_at(&w, 12, 10), Material::Soil as u8);
    assert_eq!(alpha_at(&w, 14, 10), Material::MushroomFlesh as u8);
    // an untouched empty cell reports Empty(0)
    assert_eq!(alpha_at(&w, 30, 30), Material::Empty as u8);
}

#[test]
fn burning_cell_reports_flame_code() {
    let mut w = World::new(64, 64);
    // paint oil then ignite it; a burning cell must report FLAME regardless of its base material
    w.paint(20, 20, 0, Material::Oil as u8);
    w.paint(20, 20, 0, Material::Fire as u8); // Fire itself
    w.mark_all_render_dirty();
    w.render_rgba();
    assert_eq!(alpha_at(&w, 20, 20), FLAME);
}
```

Check the exact public API names first: confirm `World::rgba()` (exists, ~line 1164), `World::render_rgba()` (~1132), `World::mark_all_render_dirty()` / equivalent, `World::paint(x,y,radius,material_u8)`, and `pub width`. If `mark_all_render_dirty` is only on the wasm wrapper, use whatever the core exposes (e.g. paint already marks dirty — then drop the explicit mark call). Adjust the test to the real core API; do not invent names.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p sandgun-core --test render`
Expected: FAIL — `burning_cell_reports_flame_code` (alpha is currently 255) and `alpha_encodes_material_id` (alpha 255 ≠ material id).

- [ ] **Step 3: Implement the alpha packing.** In `render_rgba` replace the alpha write (`self.rgba[o + 3] = 255;`) so it encodes material. Use the existing `burning` / `mat` locals already computed in that loop:

```rust
// Alpha carries the material id for the web lighting shader; burning cells / Fire report a
// synthetic FLAME code (13) so all flame lights identically. RGB is the shaded colour as before.
self.rgba[o + 3] = if burning || mat == Material::Fire { 13 } else { cell.material };
```

(`cell.material` is already the raw `u8` id. Place this where `self.rgba[o + 3] = 255;` was.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p sandgun-core --test render`
Expected: PASS (both tests).

- [ ] **Step 5: Full suite + determinism guard**

Run: `cargo test -p sandgun-core`
Expected: all green (126+2). Confirms no RNG/sim regression from the cosmetic change.

- [ ] **Step 6: Rebuild wasm & commit**

```bash
./scripts/build-wasm.sh
git add crates/sandgun-core/src/world.rs crates/sandgun-core/tests/render.rs web/src/pkg
git commit -m "feat: pack material id + FLAME code into render RGBA alpha (lighting task 1)" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```
(If `web/src/pkg` is gitignored, the `git add` of it is a no-op — that's fine; commit the rest.)

---

### Task 2: Lighting resources + emission seed pass (half-res)

**Files:**
- Modify: `web/src/renderer.js`
- Create: `web/tests-lighting/check.mjs`

**Interfaces:**
- Consumes: the world texture `ctx.tex` (RGB = colour, A = material id / FLAME, from Task 1).
- Produces: `ctx.light` = `{ lw, lh, texA, texB, fboA, fboB, seedProg, ... }` (half-res lightmap resources), and a function `seedEmission(ctx, camX, camY)` that renders per-texel **emission colour** (RGB) into `ctx.light.texA`, derived from the world texture's material alpha over the camera window. Later tasks read/propagate `texA`/`texB`.

- [ ] **Step 1: Add half-res lightmap textures + FBOs in `initGL`.** After the existing world-texture setup in `initGL` (before the `return`), create two half-resolution render targets for ping-pong and their framebuffers. Add a small helper:

```js
function makeTarget(gl, w, h, useFloat) {
  const t = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, t);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  const internal = useFloat ? gl.RGBA16F : gl.RGBA8;
  const type = useFloat ? gl.HALF_FLOAT : gl.UNSIGNED_BYTE;
  gl.texImage2D(gl.TEXTURE_2D, 0, internal, w, h, 0, gl.RGBA, type, null);
  const fbo = gl.createFramebuffer();
  gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
  gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, t, 0);
  gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  return { t, fbo };
}
```

In `initGL`, add:

```js
const useFloat = !!gl.getExtension('EXT_color_buffer_float');
const lw = Math.ceil(viewW / 2), lh = Math.ceil(viewH / 2);
const A = makeTarget(gl, lw, lh, useFloat), B = makeTarget(gl, lw, lh, useFloat);
```

Include `light: { lw, lh, useFloat, texA: A.t, fboA: A.fbo, texB: B.t, fboB: B.fbo }` in the returned ctx object. Keep the existing `prog`/`tex`/uniform fields.

- [ ] **Step 2: Add the emission seed shader + program.** At the top of `renderer.js`, add the fragment shader (reuse the existing fullscreen-triangle vertex shader `VS`, which already produces `v_uv` mapped to the world window via `u_uvOffset`/`u_uvScale`). Emission is looked up from the material id in alpha:

```glsl
// SEED_FS — emit each cell's light colour (0 for non-emitters) into the half-res lightmap.
#version 300 es
precision highp float;
uniform sampler2D u_world;   // RGB colour, A = material id (/255)
in vec2 v_uv;
out vec4 outColor;
vec3 emissionFor(int m) {
  if (m == 7)  return vec3(0.25, 1.0, 0.65);  // MushroomFlesh — hero bioluminescence
  if (m == 6)  return vec3(0.10, 0.55, 0.45); // Mycelium — dim glow
  if (m == 8)  return vec3(0.30, 0.85, 0.20); // SporeGas
  if (m == 11) return vec3(0.45, 1.0, 0.10);  // Acid
  if (m == 13) return vec3(1.0, 0.62, 0.18);  // FLAME (fire/burning)
  return vec3(0.0);
}
void main() {
  int m = int(texture(u_world, v_uv).a * 255.0 + 0.5);
  outColor = vec4(emissionFor(m), 1.0);
}
```

Compile a `seedProg` from `VS` + `SEED_FS` (reuse the `compile` helper and program-link pattern from `initGL`). Cache its uniform locations (`u_world`, `u_uvOffset`, `u_uvScale`). Store on `ctx.light.seedProg` + locs.

- [ ] **Step 3: Implement `seedEmission(ctx, camX, camY)`** (exported). Bind `fboA`, viewport to `lw×lh`, use `seedProg`, bind `ctx.tex` to unit 0, set the same `u_uvOffset=(camX/worldW, camY/worldH)` and `u_uvScale=(viewW/worldW, viewH/worldH)` used by `drawCamera`, draw the fullscreen triangle. This writes emission into `texA`.

```js
export function seedEmission(ctx, camX, camY) {
  const { gl, tex, worldW, worldH, viewW, viewH, light } = ctx;
  gl.bindFramebuffer(gl.FRAMEBUFFER, light.fboA);
  gl.viewport(0, 0, light.lw, light.lh);
  gl.useProgram(light.seedProg);
  gl.activeTexture(gl.TEXTURE0);
  gl.bindTexture(gl.TEXTURE_2D, tex);
  gl.uniform1i(light.seedWorldLoc, 0);
  gl.uniform2f(light.seedOffLoc, camX / worldW, camY / worldH);
  gl.uniform2f(light.seedScaleLoc, viewW / worldW, viewH / worldH);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
  gl.bindFramebuffer(gl.FRAMEBUFFER, null);
}
```

- [ ] **Step 4: Write the headless verification script** (`web/tests-lighting/check.mjs`). It boots the dev server page, generates a fixed seed, forces a frame, and reads back the lightmap FBO to assert emission exists where mushrooms/fire are and is zero in bare rock. Expose the light ctx for testing by adding `window.sandgun.gfx = ctx;` in `main.js` (do that wiring in Task 5; for now this script asserts the coarser "app renders, no GL errors"). Minimal first version:

```js
import pkg from '/Users/lex/.npm/_npx/e41f203b7505f1fb/node_modules/playwright/index.js';
const { chromium } = pkg;
const b = await chromium.launch(); const p = await b.newPage();
const errs = []; p.on('console', m => m.type()==='error' && errs.push(m.text()));
p.on('pageerror', e => errs.push(String(e)));
await p.goto('http://localhost:5173/');
await p.waitForFunction(() => window.sandgun?.world?.avatar_center, null, { timeout: 15000 });
await p.evaluate(() => new Promise(r => { let i=0; const t=()=>{ if(++i>=30) return r(); requestAnimationFrame(t); }; requestAnimationFrame(t); }));
const glErr = await p.evaluate(() => { const g = document.getElementById('view').getContext('webgl2'); return g.getError(); });
console.log('gl error:', glErr, '| console errors:', errs.length, errs.join(' | '));
if (glErr !== 0 || errs.length) process.exit(1);
console.log('OK'); await b.close();
```

- [ ] **Step 5: Run** — `cd web && npm run dev` (background), then `node web/tests-lighting/check.mjs`. Expected: `OK` (no GL errors, no console errors). At this point nothing visible changed on screen yet (seed pass renders to an offscreen FBO only).

- [ ] **Step 6: Commit**

```bash
git add web/src/renderer.js web/tests-lighting/check.mjs
git commit -m "feat: lightmap FBOs + emission seed pass (lighting task 2)" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Light propagation (soft occlusion diffusion, ping-pong)

**Files:**
- Modify: `web/src/renderer.js`

**Interfaces:**
- Consumes: `ctx.light.texA` (emission seed), `ctx.tex` (material alpha for occlusion).
- Produces: `propagate(ctx, camX, camY, passes)` — after it runs, the *final* lightmap (fully diffused) lives in `ctx.light.texA` (ping-pong ends there). Solid cells block light → soft shadows.

- [ ] **Step 1: Add the propagation shader.** Each pass: re-add this texel's own emission (so emitters keep glowing), plus, if this texel is transmissive, a falloff-scaled blur of its 4 (or 8) neighbours' light; if opaque, output only its own emission (light neither accumulates in nor passes through solid). Sampling `u_world` gives occlusion (material alpha) and emission; `u_light` is the previous pass.

```glsl
// PROP_FS
#version 300 es
precision highp float;
uniform sampler2D u_light;   // previous lightmap (half-res)
uniform sampler2D u_world;   // world colour+material (full-res, camera window via v_uv)
uniform vec2 u_texel;        // 1/lw, 1/lh (light space)
uniform float u_falloff;     // per-pass retention, e.g. 0.86
in vec2 v_uv;                // world-window uv (shared with seed/composite)
out vec4 outColor;
vec3 emissionFor(int m){ /* SAME body as SEED_FS emissionFor — keep identical */
  if(m==7) return vec3(0.25,1.0,0.65); if(m==6) return vec3(0.10,0.55,0.45);
  if(m==8) return vec3(0.30,0.85,0.20); if(m==11) return vec3(0.45,1.0,0.10);
  if(m==13) return vec3(1.0,0.62,0.18); return vec3(0.0); }
bool opaque(int m){ return m==1||m==2||m==5||m==7||m==10; }
void main(){
  int m = int(texture(u_world, v_uv).a*255.0 + 0.5);
  vec3 emis = emissionFor(m);
  if (opaque(m)) { outColor = vec4(emis, 1.0); return; } // solid: only its own emission, no relay
  // gather neighbours in LIGHT space; v_uv is world-window uv, but neighbour light offsets use the
  // lightmap texel size mapped back through the same window scale (see u_texel note in Step 2).
  vec3 acc = texture(u_light, v_uv).rgb * 0.4;
  acc += texture(u_light, v_uv + vec2(u_texel.x,0)).rgb * 0.15;
  acc += texture(u_light, v_uv - vec2(u_texel.x,0)).rgb * 0.15;
  acc += texture(u_light, v_uv + vec2(0,u_texel.y)).rgb * 0.15;
  acc += texture(u_light, v_uv - vec2(0,u_texel.y)).rgb * 0.15;
  outColor = vec4(max(emis, acc * u_falloff), 1.0);
}
```

Note on `u_texel`: `u_light` is sampled with the SAME `v_uv` the world uses (both cover the camera window), so a one-lightmap-texel step in that uv space is `u_texel = (uvScale.x / lw, uvScale.y / lh)` — i.e. the window's uv width divided by lightmap resolution. Compute and pass it per call. Using `max(emis, acc*falloff)` keeps emitters pinned to at least their own colour while letting diffused light fill open space.

- [ ] **Step 2: Implement `propagate(ctx, camX, camY, passes)`** — ping-pong `texA`→`texB`→`texA` … for `passes` iterations. Each iteration binds the *other* FBO, uses `propProg`, binds the previous light texture to unit 0 and the world texture to unit 1, sets `u_uvOffset`/`u_uvScale` (window), `u_texel = (uvScaleX/lw, uvScaleY/lh)`, `u_falloff`. End with the result in `texA` (if `passes` is odd, do a final copy or start so it lands in A — simplest: loop an even count, or track and expose which texture is current via `ctx.light.current`). Recommended: keep a `let src = texA/fboB` swap and after the loop set `ctx.light.result = <last written texture>` for the composite to read (don't assume A).

```js
export function propagate(ctx, camX, camY, passes) {
  const { gl, tex, worldW, worldH, viewW, viewH, light } = ctx;
  const uvsx = viewW / worldW, uvsy = viewH / worldH;
  let readT = light.texA, writeT = light.texB, writeF = light.fboB, readIsA = true;
  gl.useProgram(light.propProg);
  gl.viewport(0, 0, light.lw, light.lh);
  gl.uniform2f(light.propOffLoc, camX / worldW, camY / worldH);
  gl.uniform2f(light.propScaleLoc, uvsx, uvsy);
  gl.uniform2f(light.propTexelLoc, uvsx / light.lw, uvsy / light.lh);
  gl.uniform1f(light.propFalloffLoc, 0.86);
  for (let i = 0; i < passes; i++) {
    gl.bindFramebuffer(gl.FRAMEBUFFER, writeF);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, readT); gl.uniform1i(light.propLightLoc, 0);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D, tex);   gl.uniform1i(light.propWorldLoc, 1);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    // swap
    if (readIsA) { readT = light.texB; writeT = light.texA; writeF = light.fboA; }
    else         { readT = light.texA; writeT = light.texB; writeF = light.fboB; }
    readIsA = !readIsA;
  }
  gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  ctx.light.result = readT; // last written target is now the read target after the final swap
}
```

Verify the swap leaves `ctx.light.result` pointing at the texture that received the final draw (trace it for `passes=1` and `passes=2` before trusting it).

- [ ] **Step 3: Compile `propProg`** from `VS`+`PROP_FS` in `initGL`; cache uniform locs (`u_light`,`u_world`,`u_uvOffset`,`u_uvScale`,`u_texel`,`u_falloff`) onto `ctx.light`.

- [ ] **Step 4: Verify headless via readback.** Extend `web/tests-lighting/check.mjs` (add after the existing checks) to run seed+propagate into a scratch and read a lit pixel is brighter than a far-from-emitter pixel. This needs `window.sandgun.gfx = ctx` and small test hooks — defer the deep readback to Task 5's script when the full path is wired; for now assert no GL error after adding the passes (extend Step-2 script's frame count to 30 and re-run). Expected: still `OK`.

- [ ] **Step 5: Commit**

```bash
git add web/src/renderer.js
git commit -m "feat: soft light propagation with occlusion (lighting task 3)" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Player light + depth ambient in the composite

**Files:**
- Modify: `web/src/renderer.js`

**Interfaces:**
- Consumes: `ctx.light.result` (diffused lightmap), `ctx.tex` (world colour), avatar screen position.
- Produces: `drawLit(ctx, camX, camY, { playerX, playerY, playerRadius })` — composites the final lit scene to the default framebuffer (screen), replacing `drawCamera`. Player light is a warm additive glow around the avatar; ambient is depth-graded (bright near surface, dim deep).

- [ ] **Step 1: Add the composite shader.** Samples world colour (full-res, crisp) and the diffused light (upscaled, smooth), adds a depth ambient and a radial player light computed in-shader:

```glsl
// COMP_FS
#version 300 es
precision highp float;
uniform sampler2D u_world;    // RGB colour, A material
uniform sampler2D u_light;    // diffused lightmap (half-res, LINEAR upscaled)
uniform vec2 u_worldXY;       // window top-left in world cells (camX, camY)
uniform vec2 u_viewSize;      // viewW, viewH
uniform float u_worldH;       // total world height (cells) for depth
uniform vec2 u_player;        // avatar centre in SCREEN pixels (0..viewW, 0..viewH), or (-1) if none
uniform float u_playerR;      // player light radius in pixels
in vec2 v_uv;
out vec4 outColor;
void main() {
  vec4 world = texture(u_world, v_uv);
  vec3 light = texture(u_light, v_uv).rgb;
  // depth ambient: bright near the surface (small worldY), dim deep. worldY from the window uv.
  float worldY = u_worldXY.y + (gl_FragCoord.y / u_viewSize.y) * u_viewSize.y; // see note
  float depth = clamp(worldY / (u_worldH * 0.6), 0.0, 1.0);
  vec3 ambient = mix(vec3(0.75, 0.75, 0.80), vec3(0.06, 0.06, 0.10), depth);
  // player light: warm radial falloff in screen space
  vec3 pl = vec3(0.0);
  if (u_player.x >= 0.0) {
    float d = distance(gl_FragCoord.xy, u_player);
    float f = clamp(1.0 - d / u_playerR, 0.0, 1.0);
    pl = vec3(1.0, 0.85, 0.6) * f * f * 1.1;
  }
  vec3 lit = world.rgb * (ambient + light + pl);
  outColor = vec4(lit, 1.0);
}
```

Note on `worldY`: `gl_FragCoord.y` is bottom-origin in GL. Screen top = world row `camY`. Compute `worldY = u_worldXY.y + (1.0 - gl_FragCoord.y / u_viewSize.y) * u_viewSize.y`. Verify the top of the screen is brightest (surface) and it darkens downward — if inverted, drop the `1.0 -`.

- [ ] **Step 2: Implement `drawLit(ctx, camX, camY, opts)`** — bind default framebuffer, viewport `viewW×viewH`, use `compProg`, bind `ctx.tex` (unit 0) and `ctx.light.result` (unit 1), set `u_uvOffset`/`u_uvScale` (window, for `v_uv`), `u_worldXY=(camX,camY)`, `u_viewSize`, `u_worldH`, `u_player` (opts.playerX/Y in screen px or -1), `u_playerR` (opts.playerRadius, default e.g. 90), draw the triangle.

- [ ] **Step 3: Compile `compProg`** in `initGL`; cache locs.

- [ ] **Step 4: Verify (deferred to Task 5 wiring)** — composite isn't visible until `main.js` calls it. No standalone run here; it's exercised in Task 5.

- [ ] **Step 5: Commit**

```bash
git add web/src/renderer.js
git commit -m "feat: lit composite with depth ambient + player light (lighting task 4)" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: Wire lighting into the frame loop + debug toggle

**Files:**
- Modify: `web/src/main.js`
- Modify: `web/src/input.js`
- Modify: `web/tests-lighting/check.mjs`

**Interfaces:**
- Consumes: `seedEmission`, `propagate`, `drawLit`, `drawCamera` (kept for the toggle-off path) from `renderer.js`.
- Produces: the running game renders lit; `L` toggles lighting on/off; `window.sandgun.gfx = ctx` exposed for tests.

- [ ] **Step 1: Read `web/src/input.js`** to learn the exact key-handling pattern (how `capTicks`/regen keys are registered). Add a boolean `lightingOn` (default `true`) toggled on the `L` (`KeyL`) keydown, mirroring the existing toggle pattern (e.g. how `t` toggles `capTicks`).

- [ ] **Step 2: Update `main.js` imports and render section.** Import the new fns:

```js
import { initGL, uploadDirtyChunks, drawCamera, seedEmission, propagate, drawLit } from './renderer.js';
```

Replace the `drawCamera(ctx, Math.floor(cam.x), Math.floor(cam.y));` line (~66) with:

```js
const cx = Math.floor(cam.x), cy = Math.floor(cam.y);
if (input.lightingOn !== false) {
  seedEmission(ctx, cx, cy);
  propagate(ctx, cx, cy, 24);      // pass count = light reach; tune in Task 6
  // avatar centre -> screen pixels for the player light
  let px = -1, py = -1;
  if (c) { px = c[0] - cx; py = VIEW_H - (c[1] - cy); } // GL bottom-origin y
  drawLit(ctx, cx, cy, { playerX: px, playerY: py, playerRadius: 90 });
} else {
  drawCamera(ctx, cx, cy);
}
```

Add `window.sandgun.gfx = ctx;` next to the existing `window.sandgun = {...}` assignment so tests can reach lighting resources.

- [ ] **Step 3: Run the game** — `cd web && npm run dev`, open it. Expected: the world is now dark with a lit area around the avatar; fire/mushrooms/spore glow in colour; pressing `L` flips back to the old flat rendering. If the screen is all black: check the player-light screen-y flip and that `propagate` leaves `ctx.light.result` set. If colours look inverted vertically: fix the `worldY`/player-y flips noted in Task 4.

- [ ] **Step 4: Extend the headless check** (`web/tests-lighting/check.mjs`) to assert lighting actually darkens and colours the scene. After 60 frames, read the composited canvas via `readPixels` at two points on a fixed seed: a spot far from any emitter should be dark (low luminance), and the avatar's location should be brighter. Use `window.sandgun.gfx`/`world` to pick coordinates. Assert `farLum < nearLum` and `farLum` is well below full-bright. Keep the GL-error/console-error assertions.

- [ ] **Step 5: Run** — `node web/tests-lighting/check.mjs`. Expected: `OK` with `farLum < nearLum`.

- [ ] **Step 6: Commit**

```bash
git add web/src/main.js web/src/input.js web/tests-lighting/check.mjs
git commit -m "feat: wire lighting into the frame loop + L toggle (lighting task 5)" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 6: Flicker, tuning, and acceptance (grin test + 60fps)

**Files:**
- Modify: `web/src/renderer.js` (flicker uniform; tune constants)
- Modify: `web/src/main.js` (pass a time value for flicker; confirm fps HUD)
- Create/append: `web/tests-lighting/README.md`

**Interfaces:**
- Consumes: everything above.
- Produces: flickering fire light, tuned reach/ambient/colours, and documented acceptance.

- [ ] **Step 1: Add fire flicker.** Add a `u_time` uniform to the seed (and prop `emissionFor` is shared — simplest: apply flicker only in the seed pass by multiplying the FLAME emission by `0.8 + 0.2*sin(u_time*  freq + hash)`; a global sine is fine for v1). Pass `performance.now()/1000` from `main.js` into `seedEmission` (add a param). Keep it subtle.

- [ ] **Step 2: Tune constants** (iterate visually with the settled-world render approach — generate, step ~400, screenshot, Read it):
  - `propagate` pass count (light reach) — start 24; raise for softer/farther light, lower for tighter/cheaper.
  - `u_falloff` (0.86) — higher = light travels farther.
  - depth ambient endpoints (surface `~0.75`, deep `~0.06`) — keep caves readable but moody.
  - emission colours/intensities in `emissionFor` (both shaders must stay identical — change both).
  - player light radius (90) / colour / intensity.
  Capture 2–3 seeds after stepping the sim (so fire/fungi exist) and confirm the look.

- [ ] **Step 3: Performance check.** In-game, read the fps HUD while descending (WASD) through a lit region with fire spreading. Expected ≥60fps on the target Mac. If below: reduce pass count, ensure the lightmap is half-res, confirm no per-frame allocations were added in `renderer.js`. Record the measured fps.

- [ ] **Step 4: Grin-test acceptance (manual).** Generate a fresh world (`n`), descend into a dark cavern, and confirm: it's lit from within by bioluminescent mushrooms (cyan-green) + your own warm light; fire thrown into oil dynamically lights the room orange with soft shadows and fades as it burns out; caves are moody but readable; `L` toggles it off for comparison; ≥60fps. Capture a screenshot for the report.

- [ ] **Step 5: Docs.** `web/tests-lighting/README.md`: how to run `check.mjs` (dev server must be up; `node web/tests-lighting/check.mjs`), what it asserts, the `L` toggle, and the tunable constants + where they live. Note lighting is viewport-scale and sim-decoupled (chunk-sleep safe).

- [ ] **Step 6: Commit**

```bash
git add web/src/renderer.js web/src/main.js web/tests-lighting/README.md
git commit -m "feat: fire flicker + lighting tuning + acceptance docs (lighting task 6)" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-review notes

- **Spec coverage:** GPU viewport-only (Tasks 2–5) ✓; soft per-cell propagation w/ occlusion (Task 3) ✓; colored RGB emitters incl. bioluminescent fungi hero (Tasks 2–3 `emissionFor`) ✓; dim depth-graded ambient (Task 4) ✓; player light (Tasks 4–5) ✓; flicker (Task 6) ✓; sim untouched / chunk-sleep safe (Task 1 is cosmetic-only; all else is render layer) ✓; lighting-only scope, purely visual ✓; grin test + 60fps (Task 6) ✓. Deferred per spec: palette rework, moss/vegetation, true sky-into-mouth propagation (ambient approximates surface brightness), lighting entities (avatar overlay stays as-is, sitting in its own player light), light-driven gameplay.
- **Occlusion is soft, not exact** (diffusion, not raymarch) — per the locked design.
- **`emissionFor` is duplicated in two shaders** (SEED_FS, PROP_FS) — they MUST be edited together; Task 6 Step 2 calls this out. (A shared GLSL include isn't available without a build step; duplication is the pragmatic choice.)
- **Testability:** Task 1 is real Rust TDD. The GLSL tasks verify via `gl.getError()`, `readPixels` luminance assertions, and visual screenshots (no JS unit framework in `web/` yet; formal Playwright e2e is its own future milestone — `check.mjs` is a committed, runnable precursor).
- **Risk — result-texture tracking** in `propagate`'s ping-pong: explicitly traced in Task 3 Step 2; verify for odd/even pass counts before trusting `ctx.light.result`.
- **Risk — y-flips** (GL bottom-origin) in player-light screen coords and depth `worldY`: flagged in Tasks 4–5 with "if inverted, fix" checks.
- **Risk — float targets**: `EXT_color_buffer_float` queried with an RGBA8 fallback (light range 0..~4 fits with care); if fallback, clamp/scale is acceptable since final output is LDR anyway.
