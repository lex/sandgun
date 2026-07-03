# SANDGUN

A falling-sand physics playground. Rust sim compiled to WASM, rendered via WebGL2. See `PLAN.md` for scope and `docs/superpowers/plans/` for build milestones.

## Prerequisites

- **rustup** (not Homebrew's `rust` — the workspace needs the `wasm32-unknown-unknown` target, which only rustup can manage; `rust-toolchain.toml` pins it automatically)
- **wasm-pack** (`curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh`)
- **Node 20+** for the web front end

## Build & run

```bash
cargo test -p sandgun-core          # sim unit tests (uses Homebrew rust — fine)
./scripts/build-wasm.sh             # build the wasm pkg (handles the rustup toolchain)
cd web && npm install && npm run dev # http://localhost:5173
```

`scripts/build-wasm.sh` exists because Homebrew's `rust` can't cross-compile to wasm; it points `rustc`/`cargo` at the rustup stable toolchain (which has the `wasm32-unknown-unknown` target) while using `wasm-pack` from `~/.cargo/bin`. Rebuild after any change to `sandgun-core` or `sandgun-wasm`.

## Debug controls (in the browser)

Worlds are procedurally generated; painting is a debug tool.

- **Left-drag** — paint the selected material
- **1 / 2 / 3 / 4** — sand / water / oil / rock
- **5 / 6 / 7 / 8 / 9** — soil / mycelium / mushroom flesh / spore gas / acid
- **F** — fire &nbsp;•&nbsp; **0** or **E** — eraser
- **[** / **]** — brush radius down / up
- **N** — regenerate a new world (random seed)
- **P** — hot-reload `web/public/params.json` (tune fire/acid without rebuilding)
- **D** — toggle debug overlay (active-chunk boxes, cells-processed count)