# SANDGUN

A falling-sand physics playground. Rust sim compiled to WASM, rendered via WebGL2. See `PLAN.md` for scope and `docs/superpowers/plans/` for build milestones.

## Prerequisites

- **rustup** (not Homebrew's `rust` — the workspace needs the `wasm32-unknown-unknown` target, which only rustup can manage; `rust-toolchain.toml` pins it automatically)
- **wasm-pack** (`curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh`)
- **Node 20+** for the web front end

## Build & run

```bash
cargo test -p sandgun-core                                                    # sim unit tests
wasm-pack build crates/sandgun-wasm --release --target web --out-dir ../../web/src/pkg
cd web && npm install && npm run dev                                          # http://localhost:5173
```