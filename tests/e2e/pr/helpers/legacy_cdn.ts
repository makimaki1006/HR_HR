import * as path from 'node:path';
import type { BrowserContext } from '@playwright/test';

/**
 * 旧シェル (templates/dashboard_inline.html) は htmx と ECharts を CDN から読む。PR 時の E2E が
 * CDN の応答に左右されないよう、同じ版を root の devDependencies (package.json で exact 固定) から返す。
 * 旧シェルは SRI (integrity) 付きで読むので、版や中身がずれるとブラウザが読み込みを拒否し、
 * 旧画面側の assert が落ちる (黙って別物を使うことはない)。2026-10-01 時点で両ファイルの
 * sha384 がテンプレートの integrity と一致することを確認済み。
 */
const ROOT = path.resolve(__dirname, '../../../..'); // tests/e2e/pr/helpers → リポジトリのルート
const LOCAL: Record<string, string> = {
  'https://unpkg.com/htmx.org@2.0.4/dist/htmx.min.js': 'node_modules/htmx.org/dist/htmx.min.js',
  'https://cdn.jsdelivr.net/npm/echarts@5.5.1/dist/echarts.min.js': 'node_modules/echarts/dist/echarts.min.js',
};

export async function serveLegacyCdnLocally(context: BrowserContext): Promise<void> {
  for (const [url, file] of Object.entries(LOCAL)) {
    await context.route(url, (route) =>
      route.fulfill({
        path: path.join(ROOT, file),
        contentType: 'application/javascript',
        // crossorigin="anonymous" で読むので CORS ヘッダが要る
        headers: { 'access-control-allow-origin': '*' },
      }),
    );
  }
}
