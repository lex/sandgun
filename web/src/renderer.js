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
  outColor = vec4(emissionFor(m), 1.0);
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

  const light = {
    lw, lh, useFloat,
    texA: A.t, fboA: A.fbo, texB: B.t, fboB: B.fbo,
    seedProg, seedWorldLoc, seedOffLoc, seedScaleLoc,
  };

  gl.useProgram(prog);
  return { gl, tex, prog, worldW, worldH, viewW, viewH, chunksX, chunksY, uvOffsetLoc, uvScaleLoc, light };
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
  const { gl, prog, worldW, worldH, viewW, viewH, uvOffsetLoc, uvScaleLoc } = ctx;
  // Self-prime: set the state this draw needs regardless of what any prior pass left bound,
  // so callers can freely interleave drawCamera with other render passes (e.g. seedEmission).
  gl.useProgram(prog);
  gl.viewport(0, 0, viewW, viewH);
  gl.uniform2f(uvScaleLoc, viewW / worldW, viewH / worldH);
  gl.uniform2f(uvOffsetLoc, camX / worldW, camY / worldH);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
}

// Render per-texel emission colour (RGB) for the camera window into the half-res lightmap
// `texA`, derived from the world texture's material alpha. Renders to an offscreen FBO only --
// nothing appears on screen. Later tasks propagate/composite `texA`/`texB`.
export function seedEmission(ctx, camX, camY) {
  const { gl, tex, prog, worldW, worldH, viewW, viewH, light } = ctx;
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
  // Restore the state drawCamera (and any other main-loop pass) expects to find bound --
  // this pass only touches an offscreen FBO, so nothing here should leak into the main render.
  gl.viewport(0, 0, viewW, viewH);
  gl.useProgram(prog);
}
