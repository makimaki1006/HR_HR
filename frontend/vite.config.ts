import { fileURLToPath } from 'node:url';
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

// Tests run the legacy sales_kpi script (parity tests), whose md()/wd() read dates in the local
// time zone. Production users are in Japan, so pin the test time zone to JST. Set here (the
// config is evaluated in the main process before workers start, so they inherit it) and again
// via test.env. Works on both Windows and Linux.
process.env.TZ = 'Asia/Tokyo';

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
        crm: fromHere('src/entries/crm.tsx'),
        jobgen: fromHere('src/entries/jobgen.tsx'),
        // W8 (2026-09-29): admin (/app/admin) and my (/app/my)
        admin: fromHere('src/entries/admin.tsx'),
        my: fromHere('src/entries/my.tsx'),
        guide: fromHere('src/entries/guide.tsx'),
        // Phase 1A (2026-10-01): /app/recruitment-diag
        'recruitment-diag': fromHere('src/entries/recruitment-diag.tsx'),
        'sales-kpi': fromHere('src/entries/sales-kpi.tsx'),
      },
    },
  },
  test: {
    environment: 'node',
    env: { TZ: 'Asia/Tokyo' },
    include: ['src/**/*.test.{ts,tsx}'],
  },
});
