import { fileURLToPath } from 'node:url';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

const fromHere = (p: string): string => fileURLToPath(new URL(p, import.meta.url));

// Build output is served by Rust (Axum) from /static/app/ (ServeDir "static", immutable cache).
// File names carry content hashes; the Rust HTML shell resolves them via .vite/manifest.json.
// Everything under /static is reachable without login: never import secrets into the bundle.
export default defineConfig({
  base: '/static/app/',
  plugins: [react(), tailwindcss()],
  build: {
    outDir: fromHere('../static/app'),
    emptyOutDir: true,
    manifest: true,
    sourcemap: false,
    rolldownOptions: {
      // One entry per screen (multi-page). Add e.g. recruitmentDiag here in Phase 1A.
      input: {
        dummy: fromHere('src/entries/dummy.tsx'),
      },
    },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.{ts,tsx}'],
  },
});
