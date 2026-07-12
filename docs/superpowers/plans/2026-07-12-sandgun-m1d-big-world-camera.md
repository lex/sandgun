# SANDGUN M1d — "Big World + Camera" Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Grow the world to 1024×2048 (vertical descent) and add a smoothed follow-camera, with a dirty-chunk GPU render path so a settled, offscreen world costs ~zero to draw. Design spec: `docs/superpowers/specs/2026-07-12-m1d-big-world-camera-design.md`.

**Architecture:** The sim (`sandgun-core`) is entirely camera-agnostic — M1d is a **renderer + camera + input-mapping** change, almost all in `web/` plus a small `sandgun-core`/`sandgun-wasm` surface for dirty-chunk info and entity positions. One persistent full-world GPU texture; the fragment shader samples a camera window (panning = a free UV change); only chunks that changed re-upload. The camera lives in JS (reads `avatar_center`, lerps, drives the UV window + input mapping + entity draw).

**Tech Stack:** unchanged (Rust, wasm-bindgen/wasm-pack, Vite, WebGL2).

## Global Constraints

- Sim stays camera-agnostic: `step()`, chunk-sleep, growth, fire, gun, avatar physics all work in world coordinates and DO NOT change. Cell stays 4 bytes; NO per-cell temperature; determinism via `next_rand`/`chance`.
- Chunk sleeping is SACRED and now extends to rendering: a settled offscreen region must upload ~zero bytes/frame. The dirty-chunk render must never re-upload a chunk that didn't change; camera panning must upload nothing.
- World dims must stay multiples of `CHUNK = 64`. M1d world: **1024×2048** (16×32 = 512 chunks). Viewport (canvas) stays **640×384** cells at 1:1 (crisp pixels).
- Coordinates: world is `usize`/`isize`, `+y` down, row 0 = top. Camera offset is in world cells (`f32`). Screen→world for aim/paint = `screen/scale + camera`.
- `sandgun-wasm` stays glue-only. Commits end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` (second `-m`). Run cargo from repo root; rebuild wasm with `./scripts/build-wasm.sh`.
- **Branch:** `git checkout -b m1d-big-world` before Task 1.

## Current state (what changes)

- `web/src/main.js:8` `W=640,H=384`; `new WasmWorld(W,H)`; `initGL(view,W,H)`; `blit(ctx, whole rgba)` every frame.
- `web/src/renderer.js`: texture = `w×h`, viewport = `w×h`, fullscreen triangle samples the WHOLE texture, `blit` `texSubImage2D`s the whole buffer each frame.
- `render_rgba` (mycelium.rs / world.rs) fills the ENTIRE rgba buffer each frame and stamps entities (particles/projectiles/avatar) into it.
- `input.js`/`gun.js` `toWorld`: `(clientX-rect.left)/rect.width * worldW` — NO camera offset (fine while world==viewport).

---

### Task 1: Camera-window rendering at 1024×2048 (viewport decouple)

**Files:** Modify `web/src/main.js`, `web/src/renderer.js`. (Rust unchanged — still whole-buffer render + entity stamping; perf optimized in Task 3.)

**Interfaces produced:** `initGL(canvas, worldW, worldH, viewW, viewH)` — texture sized to world, canvas/viewport sized to view. `blit(ctx, rgbaBytes, camX, camY)` uploads the whole texture (unchanged upload for now) and draws the visible `[camX,camX+viewW]×[camY,camY+viewH]` window via UV uniforms. Camera in Task 1 is a simple centered-on-avatar snap (no smoothing yet).

- [ ] **Step 1: World to 1024×2048 + canvas stays 640×384.** In `main.js`: add `const WORLD_W = 1024, WORLD_H = 2048; const VIEW_W = 640, VIEW_H = 384;` Replace `new WasmWorld(W,H)` → `new WasmWorld(WORLD_W, WORLD_H)`, `spawn_avatar(WORLD_W/2, 4)`, `initGL(view, WORLD_W, WORLD_H, VIEW_W, VIEW_H)`, `attachInput(view, VIEW_W, VIEW_H)`, `attachGun(view, VIEW_W, VIEW_H)`. The canvas element (`index.html` `#view`/`#overlay`) width/height must be `VIEW_W×VIEW_H` (640×384) — verify index.html; the overlay canvas too.

- [ ] **Step 2: Camera-window shader.** Rewrite `renderer.js`:
```js
const VS = `#version 300 es
uniform vec2 u_uvOffset;  // top-left of the visible window in [0,1] texture space
uniform vec2 u_uvScale;   // size of the window in [0,1] texture space
out vec2 v_uv;
void main() {
  vec2 pos = vec2[3](vec2(-1,-1), vec2(3,-1), vec2(-1,3))[gl_VertexID];
  vec2 base = vec2(pos.x*0.5+0.5, pos.y*0.5+0.5);   // [0,1] across the quad
  // Translate (not mirror) the window into texture space. Screen-top (base.y=1) -> window top
  // row (v = camY/worldH); screen-bottom (base.y=0) -> camY+viewH. (1.0-base.y) flips within
  // the window without mirroring the whole texture around its midpoint.
  v_uv = vec2(u_uvOffset.x + base.x*u_uvScale.x,
              u_uvOffset.y + (1.0 - base.y)*u_uvScale.y);
  gl_Position = vec4(pos, 0.0, 1.0);
}`;
// FS unchanged (samples u_tex at v_uv)
```
`initGL(canvas, worldW, worldH, viewW, viewH)`: create the texture at `worldW×worldH` (NEAREST), `gl.viewport(0,0, viewW, viewH)`, cache uniform locations `u_uvOffset`/`u_uvScale`, and store `worldW,worldH,viewW,viewH` on `ctx`. `blit(ctx, rgbaBytes, camX, camY)`:
```js
gl.texSubImage2D(gl.TEXTURE_2D, 0, 0,0, worldW, worldH, gl.RGBA, gl.UNSIGNED_BYTE, rgbaBytes); // whole upload (Task 3 optimizes)
gl.uniform2f(uvScaleLoc, viewW/worldW, viewH/worldH);
gl.uniform2f(uvOffsetLoc, camX/worldW, camY/worldH);
gl.drawArrays(gl.TRIANGLES, 0, 3);
```

- [ ] **Step 3: Static camera in main.js.** In `frame()`, before `blit`, compute a snap camera centered on the avatar, clamped to world: read `world.avatar_center()` (returns `[x,y]` or undefined), `camX = clamp(ax - VIEW_W/2, 0, WORLD_W-VIEW_W)`, `camY = clamp(ay - VIEW_H/2, 0, WORLD_H-VIEW_H)`; pass `blit(ctx, rgba, Math.floor(camX), Math.floor(camY))`. (Smoothing is Task 2.)

- [ ] **Step 4: Verify in browser.** `cd web && npm run dev`. Expected: a 640×384 window into a 1024×2048 world; camera jumps to the avatar; you can walk/fall and the view follows (snapping). No squish, no crash. (Framerate may dip — whole-world upload/render each frame is not yet optimized; Task 3 fixes it. Note the fps for comparison.)

- [ ] **Step 5: Commit** — `git add -A && git commit -m "feat: 1024x2048 world + camera-window rendering (viewport decouple) (M1d task 1)"`

---

### Task 2: Smoothed follow-camera + camera-correct input

**Files:** Create `web/src/camera.js`; modify `web/src/main.js`, `web/src/input.js`, `web/src/gun.js`, `web/src/overlay.js`.

**Interfaces produced:** `camera.js` exports `makeCamera(viewW, viewH, worldW, worldH)` → `{x, y, update(targetCX, targetCY)}` where `x,y` are the clamped smoothed top-left in world cells. `update` lerps toward `target = avatarCenter - view/2 + downwardLead`, with a dead-zone and edge clamp. `input.js`/`gun.js` `toWorld` gain a camera offset so aim/paint hit the right world cell; `overlay.js` chunk boxes offset by the camera.

- [ ] **Step 1: camera.js.**
```js
const LERP = 0.15;          // smoothing (0..1 per frame); tune by feel
const DOWN_LEAD = 60;       // cells: bias the view downward so you see what you descend into
const DEADZONE_X = 40, DEADZONE_Y = 30; // cells: target doesn't move for small avatar moves
export function makeCamera(viewW, viewH, worldW, worldH) {
  const clampX = v => Math.max(0, Math.min(worldW - viewW, v));
  const clampY = v => Math.max(0, Math.min(worldH - viewH, v));
  const cam = { x: 0, y: 0 };
  cam.update = (acx, acy) => {
    // desired top-left so the avatar sits at (viewW/2, viewH/2 - DOWN_LEAD)
    let tx = acx - viewW / 2;
    let ty = acy - (viewH / 2 - DOWN_LEAD);
    // dead-zone: only chase once the avatar has drifted past the box from current center
    const cx = cam.x + viewW / 2, cy = cam.y + (viewH / 2 - DOWN_LEAD);
    if (Math.abs(acx - cx) < DEADZONE_X) tx = cam.x;
    if (Math.abs(acy - cy) < DEADZONE_Y) ty = cam.y;
    cam.x = clampX(cam.x + (tx - cam.x) * LERP);
    cam.y = clampY(cam.y + (ty - cam.y) * LERP);
  };
  return cam;
}
```
In `main.js`: `const cam = makeCamera(VIEW_W, VIEW_H, WORLD_W, WORLD_H);` each frame after the sim: `const c = world.avatar_center(); if (c) cam.update(c[0], c[1]);` then `blit(ctx, rgba, Math.floor(cam.x), Math.floor(cam.y))`. Pass `cam` to `applyInput`, `applyGun`, `drawOverlay`.

- [ ] **Step 2: camera-correct input.** In `input.js` and `gun.js`, `toWorld` (or the aim compute) must add the camera top-left: `worldX = (clientX-rect.left)/rect.width * VIEW_W + cam.x` (and y). Thread `cam` in via `applyInput(input, world, cam)` / `applyGun(gun, world, cam)` (or store cam on the input/gun object). Aim now hits the correct world cell regardless of scroll.

- [ ] **Step 3: overlay camera offset.** In `overlay.js`, the active-chunk boxes are drawn in world-chunk coords; offset them by `-cam.x/-cam.y` (scaled) and cull off-screen chunks so boxes line up with what's visible. Pass `cam` into `drawOverlay`.

- [ ] **Step 4: Verify in browser.** Camera smoothly trails the avatar, sits a bit high (you see below), doesn't jitter on small moves, and stops at world edges. Left-click fires at the cell under the cursor (test near screen edges + after scrolling down). Right-drag paint and the `g` debug boxes line up with the visible world.

- [ ] **Step 5: Commit** — `feat: smoothed follow-camera (dead-zone, downward lead, clamp) + camera-correct aim/paint/overlay (M1d task 2)`

---

### Task 3: Dirty-chunk GPU upload + entities as an overlay pass (the perf win)

**Files:** Modify `crates/sandgun-core/src/world.rs` (render_dirty bitmap, render_rgba dirty-only, stop stamping entities, particle accessor), `crates/sandgun-wasm/src/lib.rs` (accessors), `web/src/renderer.js` (per-chunk upload), `web/src/main.js` (upload loop + entity overlay), `web/src/materials.js` (palette for entity draw), `crates/sandgun-core/tests/`.

**Interfaces produced:** `World` gains `render_dirty: Vec<u8>` (per-chunk, set whenever a cell mutates), `render_dirty_ptr()/render_dirty_len()`, `mark_all_render_dirty()` (first frame / after generate/clear), `clear_render_dirty()`. `render_rgba` refreshes ONLY render-dirty chunk regions and NO LONGER stamps entities. New `particles_xy() -> Vec<f32>` (x,y,material triples) for the overlay. `blit_chunk(ctx, rgbaBytes, chunkX, chunkY, chunkPx)` uploads one chunk's sub-rect. Entities (particles/projectiles/avatar) drawn as a JS overlay pass each frame.

- [ ] **Step 1 (Rust): render_dirty bitmap.** Add `render_dirty: Vec<u8>` sized `chunks_x*chunks_y` (init all 1 = first-frame full upload; also set all 1 in `generate`/`clear`). In `wake(x,y)` (the single choke-point every cell mutation already calls), ALSO set `render_dirty[chunk_of(x,y)] = 1`. Add `mark_all_render_dirty()` (set all 1), `clear_render_dirty()` (set all 0), and ptr/len accessors. TDD: `render_dirty_set_on_mutation` (paint a cell → its chunk dirty), `render_dirty_clear` (clear → all 0), `settled_world_has_no_render_dirty_after_clear` (settle, clear, step once with nothing moving → still all 0). 

- [ ] **Step 2 (Rust): render_rgba dirty-only + stop stamping entities.** Change `render_rgba` to iterate ONLY chunks with `render_dirty==1`, rewriting those cells' RGBA (grid materials only). REMOVE the particle/projectile/avatar stamping from `render_rgba` (they move to the JS overlay). Keep the CPU rgba buffer persistent (it already is). Add `particles_xy() -> Vec<f32>` (x, y, material for each live particle) on World + wasm passthrough; `projectiles_xy()` + `avatar_xywh()` already exist. TDD: `render_rgba_only_touches_dirty_chunks` (dirty one chunk, render, assert cells outside it unchanged from a known prior state), and update/keep the existing render test (a painted cell shows its palette color).

- [ ] **Step 3 (JS): per-chunk upload.** In `renderer.js` add `blit_chunk`/a dirty-upload helper: given the render-dirty bitmap + the full rgba buffer, for each dirty chunk `texSubImage2D` its 64×64 sub-rect (`gl.texSubImage2D(TEXTURE_2D,0, cx*64, cy*64, w, h, RGBA, UNSIGNED_BYTE, <chunk view>)` — note WebGL2 can upload a sub-rect from a larger buffer via `gl.pixelStorei(UNPACK_ROW_LENGTH, worldW)` + offset, OR pack per-chunk rows; use `UNPACK_ROW_LENGTH=worldW`, `UNPACK_SKIP_PIXELS=cx*64`, `UNPACK_SKIP_ROWS=cy*64` then upload w×h from the full buffer). Then `drawArrays` the camera window as before. In `main.js`, replace `blit(whole)` with: read render_dirty bitmap (fresh Uint8Array view each frame — wasm memory can grow), upload dirty chunks, then `world.clear_render_dirty()`. Camera pan still uploads nothing (only dirty chunks upload).

- [ ] **Step 4 (JS): entity overlay pass.** After the world `drawArrays`, draw entities on top. Simplest: draw on the existing 2D `#overlay` canvas (octx) each frame (already cleared in drawOverlay) — for each particle (`particles_xy`), projectile (`projectiles_xy`), and the avatar (`avatar_xywh`), fillRect at `(worldX-cam.x, worldY-cam.y)` scaled to canvas, culling off-screen. Mirror the 13-material palette in `materials.js` for particle colors (projectiles = the hot tracer color; avatar = its cyan). Ensure the overlay draw happens after the debug HUD/boxes or is layered sensibly.

- [ ] **Step 5: Verify.** `cargo test -p sandgun-core` green; `./scripts/build-wasm.sh`. In browser: world renders identically, particles/projectiles/avatar visible and move smoothly (no trails — the overlay redraws fresh each frame), and with `g` debug on, a static screen shows dirty-chunk count drop to ~0 (only entity-occupied areas / active sim). FPS should recover vs Task 1/2.

- [ ] **Step 6: Commit** — `feat: dirty-chunk GPU upload + entity overlay pass — settled world uploads ~zero (M1d task 3)`

---

### Task 4: Render-cost HUD + M1d acceptance

**Files:** Modify `web/src/overlay.js` (render stats), `crates/sandgun-wasm` if a count helper is cleaner; browser acceptance.

- [ ] **Step 1: HUD render stats.** Extend the debug overlay (behind `g`) to show, alongside fps/rate/growth: **dirty chunks uploaded this frame** and **camera (x,y)**. Compute uploaded-chunk count in JS from the render_dirty bitmap before clearing. This makes the kill criterion observable.

- [ ] **Step 2: Browser acceptance (the M1d kill criterion).** `cd web && npm run dev`. Drive headless (Playwright at `/Users/lex/.npm/_npx/e41f203b7505f1fb/node_modules/playwright`, CJS import) OR document a manual checklist. Verify: (1) walk/fall from the top of the 1024×2048 world to the bottom (drive avatar input or teleport-by-input over time), camera following, shooting + growing mycelium along the way; (2) FPS holds ~60 on the descent (capture numbers); (3) with the avatar standing still in a settled region, dirty-chunks-uploaded drops to ~0 (settled offscreen world costs ~zero to render) — the core proof; (4) camera pan alone (avatar moving through static terrain) uploads ~0 chunks; (5) aim maps correctly at the bottom of the world (camera offset correct at scale); (6) no console errors, no visual seams/tearing at chunk boundaries. Capture observations / FPS.

- [ ] **Step 3: Commit** — `feat: render-cost HUD + M1d acceptance (task 4)`

---

## Self-review notes

- Spec coverage: 1024×2048 ✓ (T1); dirty-chunk upload / settled≈zero ✓ (T3); camera window + free pan ✓ (T1 shader, T3 no-upload-on-pan); follow-cam smoothed + downward lead + dead-zone + clamp ✓ (T2); ~640×384 viewport at 1:1 ✓ (T1); worldgen scaled, no formations ✓ (uses existing generate at 2048); kill criterion = descend at 60fps + settled≈zero ✓ (T4).
- **Key engineering call (flagged):** entities move from render_rgba stamping to a JS **overlay pass** (T3), because stamping moving entities into a *persistent* dirty-chunk texture would leave trails (an entity's vacated chunk isn't re-dirtied). Overlay draw redraws them fresh each frame and decouples them from the world texture — also the right setup for the future animated-avatar sprite. Alternative (keep stamping, dirty each entity's current+previous chunk) is fiddlier; overlay chosen. If the reviewer/Lex prefers stamping, T3 changes.
- Sim untouched: only rendering + input mapping + a render_dirty bitmap (piggybacking `wake()`) and read-only accessors are added to core. Chunk-sleep/determinism unaffected.
- Deferred: predefined-formation worldgen, zoom, bigger/animated avatar sprite, streaming — all separate backlog items.
- Perf note: Tasks 1–2 still upload the whole 8 MB texture each frame (may dip below 60fps at 2M cells); that's an accepted intermediate — Task 3 is the fix and Task 4 is the gate.
