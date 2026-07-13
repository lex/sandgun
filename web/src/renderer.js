const VS = `#version 300 es
uniform vec2 u_uvOffset;  // top-left of the visible window in [0,1] texture space
uniform vec2 u_uvScale;   // size of the window in [0,1] texture space
out vec2 v_uv;
void main() {
  // fullscreen triangle from gl_VertexID, no buffers needed
  vec2 pos = vec2[3](vec2(-1, -1), vec2(3, -1), vec2(-1, 3))[gl_VertexID];
  vec2 base = vec2(pos.x * 0.5 + 0.5, pos.y * 0.5 + 0.5); // [0,1] across the quad
  // Translate (not mirror) the visible window into texture space. World row 0 is the top and
  // texture row 0 is uploaded first, so screen-top (base.y=1) must map to the window's top row
  // (v = camY/worldH) and screen-bottom (base.y=0) to camY+viewH. Using (1.0 - base.y) flips
  // within the window without mirroring the whole texture around its midpoint.
  v_uv = vec2(u_uvOffset.x + base.x * u_uvScale.x,
              u_uvOffset.y + (1.0 - base.y) * u_uvScale.y);
  gl_Position = vec4(pos, 0.0, 1.0);
}`;

const FS = `#version 300 es
precision mediump float;
uniform sampler2D u_tex;
in vec2 v_uv;
out vec4 outColor;
void main() { outColor = texture(u_tex, v_uv); }`;

// SEED_FS -- emit each cell's light colour (0 for non-emitters) into the half-res lightmap.
const SEED_FS = `#version 300 es
precision highp float;
uniform sampler2D u_world;   // RGB colour, A = material id (/255)
uniform float u_time;        // seconds; drives fire flicker (seed stage only)
in vec2 v_uv;
out vec4 outColor;
vec3 emissionFor(int m) {
  if (m == 7)  return vec3(0.25, 1.0, 0.65);  // MushroomFlesh -- hero bioluminescence
  if (m == 6)  return vec3(0.10, 0.55, 0.45); // Mycelium -- dim glow
  if (m == 8)  return vec3(0.30, 0.85, 0.20); // SporeGas
  if (m == 11) return vec3(0.45, 1.0, 0.10);  // Acid
  if (m == 13) return vec3(1.0, 0.62, 0.18);  // FLAME (fire/burning)
  return vec3(0.0);
}
void main() {
  int m = int(texture(u_world, v_uv).a * 255.0 + 0.5);
  vec3 emis = emissionFor(m);
  // Fire (FLAME=13) shimmers spatially + temporally so flames feel alive without a global
  // pulse. Applied ONLY here in the seed stage -- emissionFor stays byte-identical to PROP_FS.
  if (m == 13) emis *= 0.82 + 0.18 * sin(u_time * 9.0 + v_uv.x * 40.0 + v_uv.y * 40.0);
  outColor = vec4(emis, 1.0);
}`;

// PROP_FS -- one diffusion pass. Reads this texel's material (u_world alpha) for occlusion +
// emission, reads the previous lightmap (u_light) for the neighbour blur. Opaque cells emit only
// their own colour and never relay (soft shadows); transmissive cells take max(own emission,
// falloff * 5-tap blur of neighbours) so emitters stay pinned while light fills open space.
// NOTE: emissionFor MUST stay byte-identical to SEED_FS.emissionFor (tuned together later).
const PROP_FS = `#version 300 es
precision highp float;
uniform sampler2D u_light;   // previous lightmap (half-res)
uniform sampler2D u_world;   // world colour + material (full-res, camera window via v_uv)
uniform vec2 u_res;          // lightmap resolution (lw, lh) -- for screen-space neighbour reads
uniform float u_falloff;     // per-pass retention, e.g. 0.86
in vec2 v_uv;
out vec4 outColor;
vec3 emissionFor(int m) {
  if (m == 7)  return vec3(0.25, 1.0, 0.65);  // MushroomFlesh -- hero bioluminescence
  if (m == 6)  return vec3(0.10, 0.55, 0.45); // Mycelium -- dim glow
  if (m == 8)  return vec3(0.30, 0.85, 0.20); // SporeGas
  if (m == 11) return vec3(0.45, 1.0, 0.10);  // Acid
  if (m == 13) return vec3(1.0, 0.62, 0.18);  // FLAME (fire/burning)
  return vec3(0.0);
}
bool opaque(int m) { return m == 1 || m == 2 || m == 5 || m == 7 || m == 10; }
void main() {
  int m = int(texture(u_world, v_uv).a * 255.0 + 0.5);
  vec3 emis = emissionFor(m);
  if (opaque(m)) { outColor = vec4(emis, 1.0); return; } // solid: only own emission, no relay
  // The lightmap is SCREEN-indexed (written by a fullscreen pass into the half-res FBO), so its
  // neighbours live at screen coords, NOT world-window v_uv. This prop pass renders at lw x lh,
  // so gl_FragCoord.xy ranges [0,lw]x[0,lh] and luv is [0,1] screen space matching the write.
  vec2 luv = gl_FragCoord.xy / u_res;
  vec3 acc = texture(u_light, luv).rgb * 0.4;
  acc += texture(u_light, luv + vec2(1.0/u_res.x, 0.0)).rgb * 0.15;
  acc += texture(u_light, luv - vec2(1.0/u_res.x, 0.0)).rgb * 0.15;
  acc += texture(u_light, luv + vec2(0.0, 1.0/u_res.y)).rgb * 0.15;
  acc += texture(u_light, luv - vec2(0.0, 1.0/u_res.y)).rgb * 0.15;
  outColor = vec4(max(emis, acc * u_falloff), 1.0);
}`;

// COMP_FS -- the LIT COMPOSITE pass. Samples the crisp full-res world colour (v_uv, world-texture
// space) and the smooth half-res diffused lightmap, then multiplies world colour by (depth ambient
// + diffused light + player light). Depth ambient is bright near the surface (small worldY) and
// dims deep; the player light is a warm additive radial glow in screen space.
// NOTE: the lightmap is WINDOW-scale (its 0..1 uv spans the visible view, bottom-origin), NOT the
// world texture, so it must be sampled with the screen-normalized coord (gl_FragCoord.xy/viewSize),
// the same frame the depth/player terms use -- sampling it with the world-space v_uv reads the
// wrong cell (offset by camX/worldW etc.) and the emission never lands.
const COMP_FS = `#version 300 es
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
  vec3 light = texture(u_light, gl_FragCoord.xy / u_viewSize).rgb; // window-scale lightmap, not v_uv
  // depth ambient: bright near the surface (small worldY), dim deep. gl_FragCoord.y is
  // bottom-origin in GL, so screen top (world row camY) is the max fragcoord y -- invert it.
  float worldY = u_worldXY.y + (1.0 - gl_FragCoord.y / u_viewSize.y) * u_viewSize.y;
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
}`;

// Chunk size in world cells -- must match sandgun-core's `world::CHUNK`.
const CHUNK = 64;

function compile(gl, type, src) {
  const s = gl.createShader(type);
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(s));
  }
  return s;
}

export function initGL(canvas, worldW, worldH, viewW, viewH) {
  const gl = canvas.getContext('webgl2');
  if (!gl) throw new Error('WebGL2 required');
  const prog = gl.createProgram();
  gl.attachShader(prog, compile(gl, gl.VERTEX_SHADER, VS));
  gl.attachShader(prog, compile(gl, gl.FRAGMENT_SHADER, FS));
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(prog));
  }
  gl.useProgram(prog);
  gl.bindVertexArray(gl.createVertexArray());
  const tex = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, tex);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, worldW, worldH, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
  gl.viewport(0, 0, viewW, viewH);
  const uvOffsetLoc = gl.getUniformLocation(prog, 'u_uvOffset');
  const uvScaleLoc = gl.getUniformLocation(prog, 'u_uvScale');
  const texLoc = gl.getUniformLocation(prog, 'u_tex');
  const chunksX = Math.ceil(worldW / CHUNK);
  const chunksY = Math.ceil(worldH / CHUNK);

  // --- Lighting resources: two half-res ping-pong render targets + emission seed pass. ---
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
    if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) {
      console.warn('lightmap FBO incomplete');
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    return { t, fbo };
  }
  const useFloat = !!gl.getExtension('EXT_color_buffer_float');
  const lw = Math.ceil(viewW / 2), lh = Math.ceil(viewH / 2);
  const A = makeTarget(gl, lw, lh, useFloat), B = makeTarget(gl, lw, lh, useFloat);

  const seedProg = gl.createProgram();
  gl.attachShader(seedProg, compile(gl, gl.VERTEX_SHADER, VS));
  gl.attachShader(seedProg, compile(gl, gl.FRAGMENT_SHADER, SEED_FS));
  gl.linkProgram(seedProg);
  if (!gl.getProgramParameter(seedProg, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(seedProg));
  }
  const seedWorldLoc = gl.getUniformLocation(seedProg, 'u_world');
  const seedOffLoc = gl.getUniformLocation(seedProg, 'u_uvOffset');
  const seedScaleLoc = gl.getUniformLocation(seedProg, 'u_uvScale');
  const seedTimeLoc = gl.getUniformLocation(seedProg, 'u_time');

  const propProg = gl.createProgram();
  gl.attachShader(propProg, compile(gl, gl.VERTEX_SHADER, VS));
  gl.attachShader(propProg, compile(gl, gl.FRAGMENT_SHADER, PROP_FS));
  gl.linkProgram(propProg);
  if (!gl.getProgramParameter(propProg, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(propProg));
  }
  const propLightLoc = gl.getUniformLocation(propProg, 'u_light');
  const propWorldLoc = gl.getUniformLocation(propProg, 'u_world');
  const propOffLoc = gl.getUniformLocation(propProg, 'u_uvOffset');
  const propScaleLoc = gl.getUniformLocation(propProg, 'u_uvScale');
  const propResLoc = gl.getUniformLocation(propProg, 'u_res');
  const propFalloffLoc = gl.getUniformLocation(propProg, 'u_falloff');

  // Composite program: world colour x (ambient + diffused light + player light) -> screen.
  const compProg = gl.createProgram();
  gl.attachShader(compProg, compile(gl, gl.VERTEX_SHADER, VS));
  gl.attachShader(compProg, compile(gl, gl.FRAGMENT_SHADER, COMP_FS));
  gl.linkProgram(compProg);
  if (!gl.getProgramParameter(compProg, gl.LINK_STATUS)) {
    throw new Error(gl.getProgramInfoLog(compProg));
  }
  const compWorldLoc = gl.getUniformLocation(compProg, 'u_world');
  const compLightLoc = gl.getUniformLocation(compProg, 'u_light');
  const compOffLoc = gl.getUniformLocation(compProg, 'u_uvOffset');
  const compScaleLoc = gl.getUniformLocation(compProg, 'u_uvScale');
  const compWorldXYLoc = gl.getUniformLocation(compProg, 'u_worldXY');
  const compViewSizeLoc = gl.getUniformLocation(compProg, 'u_viewSize');
  const compWorldHLoc = gl.getUniformLocation(compProg, 'u_worldH');
  const compPlayerLoc = gl.getUniformLocation(compProg, 'u_player');
  const compPlayerRLoc = gl.getUniformLocation(compProg, 'u_playerR');

  const light = {
    lw, lh, useFloat,
    texA: A.t, fboA: A.fbo, texB: B.t, fboB: B.fbo,
    seedProg, seedWorldLoc, seedOffLoc, seedScaleLoc, seedTimeLoc,
    propProg, propLightLoc, propWorldLoc, propOffLoc, propScaleLoc, propResLoc, propFalloffLoc,
    compProg, compWorldLoc, compLightLoc, compOffLoc, compScaleLoc,
    compWorldXYLoc, compViewSizeLoc, compWorldHLoc, compPlayerLoc, compPlayerRLoc,
  };

  gl.useProgram(prog);
  return { gl, tex, prog, worldW, worldH, viewW, viewH, chunksX, chunksY, uvOffsetLoc, uvScaleLoc, texLoc, light };
}

// Upload only the chunks flagged dirty in `dirty` (a Uint8Array, one byte per chunk, row-major
// over chunksX*chunksY) to their 64x64 sub-rects of the world texture, reading straight out of
// the full persistent `rgbaBytes` buffer via WebGL2's UNPACK_ROW_LENGTH/SKIP_PIXELS/SKIP_ROWS --
// no per-chunk copy needed. A settled world (dirty all-zero) uploads nothing. Returns the
// number of chunks uploaded (handy for a HUD counter).
export function uploadDirtyChunks(ctx, rgbaBytes, dirty) {
  const { gl, tex, worldW, worldH, chunksX, chunksY } = ctx;
  gl.bindTexture(gl.TEXTURE_2D, tex);
  gl.pixelStorei(gl.UNPACK_ROW_LENGTH, worldW);
  let uploaded = 0;
  for (let cy = 0; cy < chunksY; cy++) {
    const y0 = cy * CHUNK;
    const h = Math.min(CHUNK, worldH - y0);
    for (let cx = 0; cx < chunksX; cx++) {
      if (!dirty[cy * chunksX + cx]) continue;
      const x0 = cx * CHUNK;
      const w = Math.min(CHUNK, worldW - x0);
      gl.pixelStorei(gl.UNPACK_SKIP_PIXELS, x0);
      gl.pixelStorei(gl.UNPACK_SKIP_ROWS, y0);
      gl.texSubImage2D(gl.TEXTURE_2D, 0, x0, y0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, rgbaBytes);
      uploaded++;
    }
  }
  // Reset unpack state -- other texture uploads (or a future caller) must not inherit it.
  gl.pixelStorei(gl.UNPACK_ROW_LENGTH, 0);
  gl.pixelStorei(gl.UNPACK_SKIP_PIXELS, 0);
  gl.pixelStorei(gl.UNPACK_SKIP_ROWS, 0);
  return uploaded;
}

// Draw the camera window (the visible [camX,camX+viewW]x[camY,camY+viewH] slice of the world
// texture) -- no upload here; call uploadDirtyChunks first.
export function drawCamera(ctx, camX, camY) {
  const { gl, tex, prog, worldW, worldH, viewW, viewH, uvOffsetLoc, uvScaleLoc, texLoc } = ctx;
  // Self-prime: set the state this draw needs regardless of what any prior pass left bound,
  // so callers can freely interleave drawCamera with other render passes (e.g. seedEmission).
  gl.useProgram(prog);
  gl.viewport(0, 0, viewW, viewH);
  gl.activeTexture(gl.TEXTURE0);
  gl.bindTexture(gl.TEXTURE_2D, tex);
  gl.uniform1i(texLoc, 0);
  gl.uniform2f(uvScaleLoc, viewW / worldW, viewH / worldH);
  gl.uniform2f(uvOffsetLoc, camX / worldW, camY / worldH);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
}

// Render per-texel emission colour (RGB) for the camera window into the half-res lightmap
// `texA`, derived from the world texture's material alpha. Renders to an offscreen FBO only --
// nothing appears on screen. Later tasks propagate/composite `texA`/`texB`. `time` (seconds)
// drives a subtle FLAME flicker applied on top of emissionFor; omit/0 for a steady seed.
export function seedEmission(ctx, camX, camY, time) {
  const { gl, tex, prog, worldW, worldH, viewW, viewH, light } = ctx;
  gl.bindFramebuffer(gl.FRAMEBUFFER, light.fboA);
  gl.viewport(0, 0, light.lw, light.lh);
  gl.useProgram(light.seedProg);
  gl.activeTexture(gl.TEXTURE0);
  gl.bindTexture(gl.TEXTURE_2D, tex);
  gl.uniform1i(light.seedWorldLoc, 0);
  gl.uniform2f(light.seedOffLoc, camX / worldW, camY / worldH);
  gl.uniform2f(light.seedScaleLoc, viewW / worldW, viewH / worldH);
  gl.uniform1f(light.seedTimeLoc, time || 0); // drives FLAME flicker; 0 = steady if omitted
  gl.drawArrays(gl.TRIANGLES, 0, 3);
  gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  // Restore the state drawCamera (and any other main-loop pass) expects to find bound --
  // this pass only touches an offscreen FBO, so nothing here should leak into the main render.
  gl.viewport(0, 0, viewW, viewH);
  gl.useProgram(prog);
  gl.activeTexture(gl.TEXTURE0);
  gl.bindTexture(gl.TEXTURE_2D, tex);
}

// Diffuse the seeded emission (in texA) across the lightmap for `passes` iterations, ping-ponging
// texA<->texB. Each pass spreads light to open neighbours and is blocked by solid terrain (soft
// occlusion shadows), re-adding each cell's own emission so emitters keep glowing. Renders to the
// offscreen half-res targets only. After it runs, `ctx.light.result` points at the texture that
// received the final draw (read that in the composite -- do NOT assume texA). Self-primes and
// restores GL state (viewport + main program + unbound FBO), matching seedEmission's pattern.
export function propagate(ctx, camX, camY, passes) {
  const { gl, tex, prog, worldW, worldH, viewW, viewH, light } = ctx;
  const uvsx = viewW / worldW, uvsy = viewH / worldH;
  // Ping-pong: read the current lightmap, write the other. After each draw we swap, so `readT`
  // always names the texture that was just written -- hence `ctx.light.result = readT` at the end.
  let readT = light.texA, writeF = light.fboB, readIsA = true;
  gl.useProgram(light.propProg);
  gl.viewport(0, 0, light.lw, light.lh);
  gl.uniform2f(light.propOffLoc, camX / worldW, camY / worldH);
  gl.uniform2f(light.propScaleLoc, uvsx, uvsy);
  gl.uniform2f(light.propResLoc, light.lw, light.lh);
  gl.uniform1f(light.propFalloffLoc, 0.90);
  for (let i = 0; i < passes; i++) {
    gl.bindFramebuffer(gl.FRAMEBUFFER, writeF);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, readT); gl.uniform1i(light.propLightLoc, 0);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D, tex);   gl.uniform1i(light.propWorldLoc, 1);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    // Swap: the texture we just wrote (attached to writeF) becomes the next read source.
    if (readIsA) { readT = light.texB; writeF = light.fboA; }
    else         { readT = light.texA; writeF = light.fboB; }
    readIsA = !readIsA;
  }
  ctx.light.result = readT; // last-written texture (readT names it after the final swap)
  // Restore state for the main render path (seedEmission-style self-priming).
  gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  gl.viewport(0, 0, viewW, viewH);
  gl.useProgram(prog);
  gl.activeTexture(gl.TEXTURE0);
  gl.bindTexture(gl.TEXTURE_2D, tex);
}

// LIT COMPOSITE: draw the final lit scene (world colour x (depth ambient + diffused light +
// player light)) to the default framebuffer (screen). Run seedEmission + propagate first so
// `ctx.light.result` names the diffused lightmap. `opts = { playerX, playerY, playerRadius }`
// with the avatar centre in TOP-DOWN SCREEN pixels (0,0 = top-left of the view, y grows downward
// -- the same convention as input.js's clientY - rect.top and every other screen coordinate in
// this codebase); pass no player (or playerX < 0) to skip the glow. drawLit converts playerY to
// GL's bottom-origin gl_FragCoord frame internally before upload, so callers never need to flip.
// Self-primes its own program, viewport, and textures (world @ unit 0, lightmap @ unit 1) and
// restores the established GL-state contract on exit so a later drawCamera / seed / propagate
// still works.
export function drawLit(ctx, camX, camY, opts) {
  const { gl, tex, prog, worldW, worldH, viewW, viewH, light } = ctx;
  const o = opts || {};
  const hasPlayer = o.playerX != null && o.playerY != null && o.playerX >= 0;
  gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  gl.viewport(0, 0, viewW, viewH);
  gl.useProgram(light.compProg);
  // World colour (full-res, crisp) on unit 0.
  gl.activeTexture(gl.TEXTURE0);
  gl.bindTexture(gl.TEXTURE_2D, tex);
  gl.uniform1i(light.compWorldLoc, 0);
  // Diffused lightmap (half-res, LINEAR upscaled) on unit 1.
  gl.activeTexture(gl.TEXTURE1);
  gl.bindTexture(gl.TEXTURE_2D, light.result);
  gl.uniform1i(light.compLightLoc, 1);
  // Camera window into texture space (same formulas as drawCamera, drives v_uv).
  gl.uniform2f(light.compScaleLoc, viewW / worldW, viewH / worldH);
  gl.uniform2f(light.compOffLoc, camX / worldW, camY / worldH);
  gl.uniform2f(light.compWorldXYLoc, camX, camY);
  gl.uniform2f(light.compViewSizeLoc, viewW, viewH);
  gl.uniform1f(light.compWorldHLoc, worldH);
  // u_player feeds gl_FragCoord.xy in COMP_FS, which is bottom-origin in GL -- flip the
  // top-down screen y we accept from callers into that frame here at the upload boundary.
  if (hasPlayer) gl.uniform2f(light.compPlayerLoc, o.playerX, viewH - o.playerY);
  else gl.uniform2f(light.compPlayerLoc, -1, -1);
  gl.uniform1f(light.compPlayerRLoc, o.playerRadius != null ? o.playerRadius : 90);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
  // Restore the GL-state contract: main program + unit-0 world texture bound, viewport intact,
  // default framebuffer left bound (it is the screen). Unit 1 is left as-is; callers that need
  // it re-bind their own (propagate does), matching the self-prime discipline.
  gl.useProgram(prog);
  gl.activeTexture(gl.TEXTURE0);
  gl.bindTexture(gl.TEXTURE_2D, tex);
}
