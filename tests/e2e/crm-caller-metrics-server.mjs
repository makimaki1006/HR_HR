// Local component harness: fictional "today's call metrics" strip (CRM) and admin table; no Rust startup or credentials.
import { createServer } from '../../frontend/node_modules/vite/dist/node/index.js';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('../../frontend', import.meta.url));
const pages = {
  '/static/app/__metrics_crm': '/src/screens/crm/savedViewsPreview.test-fixture.tsx',
  '/static/app/__metrics_admin': '/src/screens/admin/callersPreview.test-fixture.tsx',
};
const server = await createServer({
  root, configFile: fileURLToPath(new URL('../../frontend/vite.config.ts', import.meta.url)),
  server: { host: '127.0.0.1', port: 19346, strictPort: true },
  plugins: [{
    name: 'caller-metrics-test-harness',
    configureServer(vite) {
      vite.middlewares.use(async (req, res, next) => {
        const path = (req.url ?? '').split('?')[0];
        const entry = pages[path];
        if (!entry) return next();
        const html = await vite.transformIndexHtml(req.url, `<!doctype html><html lang="ja"><head><meta charset="utf-8"><title>今日の成績の確認</title></head><body><div id="root"></div><script type="module" src="${entry}"></script></body></html>`);
        res.setHeader('Content-Type', 'text/html'); res.end(html);
      });
    },
  }],
});
await server.listen();
