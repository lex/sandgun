export function drawOverlay(octx, world, wasm, input, fps, gun) {
  const { width: w, height: h } = octx.canvas;
  octx.clearRect(0, 0, w, h);
  octx.font = '10px monospace';
  octx.fillStyle = '#9f9';
  const rate = input.capTicks ? '60hz' : 'uncap';
  let growth = '';
  if (input.debug) {
    growth = ` · colonies ${world.colony_count()} · tips ${world.tip_count()} · mush ${world.mushroom_count()}`;
  }
  octx.fillText(`${fps.toFixed(0)} fps · ${rate} · ${input.status}${gun ? ` · ${gun.status}` : ''}${input.debug ? ` · ${world.cells_processed()} cells${growth}` : ''}`, 6, 12);
  if (!input.debug) return;
  const cw = w / world.chunks_x(), ch = h / world.chunks_y();
  const active = new Uint8Array(wasm.memory.buffer, world.active_ptr(), world.active_len());
  octx.strokeStyle = 'rgba(255, 60, 60, 0.8)';
  for (let cy = 0; cy < world.chunks_y(); cy++) {
    for (let cx = 0; cx < world.chunks_x(); cx++) {
      if (active[cy * world.chunks_x() + cx]) {
        octx.strokeRect(cx * cw + 0.5, cy * ch + 0.5, cw - 1, ch - 1);
      }
    }
  }
}
