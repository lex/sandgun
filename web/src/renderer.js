const VS = `#version 300 es
out vec2 v_uv;
void main() {
  // fullscreen triangle from gl_VertexID, no buffers needed
  vec2 pos = vec2[3](vec2(-1, -1), vec2(3, -1), vec2(-1, 3))[gl_VertexID];
  // world row 0 is the top; texture row 0 is uploaded first -> flip v
  v_uv = vec2(pos.x * 0.5 + 0.5, 1.0 - (pos.y * 0.5 + 0.5));
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

export function initGL(canvas, w, h) {
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
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
  gl.viewport(0, 0, w, h);
  return { gl, w, h };
}

export function blit(ctx, rgbaBytes) {
  const { gl, w, h } = ctx;
  gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, rgbaBytes);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
}
