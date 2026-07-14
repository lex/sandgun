import { defineConfig } from 'vite';

// GitHub Pages serves this project under https://lex.github.io/sandgun/, so all
// assets (JS, the wasm blob, params.json) must resolve under the /sandgun/ prefix.
// Locally `vite dev` and `vite build` still work because base only prefixes paths.
// Override with SANDGUN_BASE=/ for a root-hosted deploy (custom domain, etc.).
export default defineConfig({
  base: process.env.SANDGUN_BASE ?? '/sandgun/',
  // main.js uses top-level await to init the wasm module; esnext keeps it instead
  // of down-compiling to an unsupported target. Modern browsers all support it.
  build: { target: 'esnext' },
});
