// Local component harness: fake email cards; no Rust startup or external credentials.
import { createServer } from '../../frontend/node_modules/vite/dist/node/index.js';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('../../frontend', import.meta.url));
const server = await createServer({
  root, configFile: fileURLToPath(new URL('../../frontend/vite.config.ts', import.meta.url)),
  server: { host: '127.0.0.1', port: 19343, strictPort: true },
  plugins: [{
    name: 'property-layout-test-harness',
    configureServer(vite) {
      vite.middlewares.use(async (req, res, next) => {
        if (!req.url?.startsWith('/static/app/__layout_preview')) return next();
        const html = await vite.transformIndexHtml(req.url, '<!doctype html><html lang="ja"><head><meta charset="utf-8"><title>メール表示確認</title></head><body><div id="root"></div><script type="module" src="/src/screens/crm/propertyLayoutPreview.test-fixture.tsx"></script></body></html>');
        res.setHeader('Content-Type', 'text/html'); res.end(html);
      });
    },
  }],
});
await server.listen();
