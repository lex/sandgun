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
  return { gl, worldW, worldH, viewW, viewH, uvOffsetLoc, uvScaleLoc };
}

export function blit(ctx, rgbaBytes, camX, camY) {
  const { gl, worldW, worldH, viewW, viewH, uvOffsetLoc, uvScaleLoc } = ctx;
  gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, worldW, worldH, gl.RGBA, gl.UNSIGNED_BYTE, rgbaBytes);
  gl.uniform2f(uvScaleLoc, viewW / worldW, viewH / worldH);
  gl.uniform2f(uvOffsetLoc, camX / worldW, camY / worldH);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
}
