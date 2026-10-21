import { useState } from 'react';
import { ROUTES, usePreviewNav } from '../../preview';
import { AuthLayout } from '../../ui/layouts';
import { Button } from '../../ui/kit';
import { BTN_LG, Heading, IconCopy } from './parts';

import { LOGIN_URL } from '../../fixtures';

const PHONE_URL = LOGIN_URL;
const SHORT_URL = LOGIN_URL.replace('https://', '');
const FINDER = [{ fx: 0, fy: 0 }, { fx: 18, fy: 0 }, { fx: 0, fy: 18 }];

/** 見た目だけの QR コード（試作）。 */
function FakeQr() {
  const cells: { x: number; y: number }[] = [];
  for (let y = 0; y < 25; y += 1) {
    for (let x = 0; x < 25; x += 1) {
      const inFinder = FINDER.some(({ fx, fy }) => x >= fx && x < fx + 7 && y >= fy && y < fy + 7);
      if (!inFinder && (x * 7 + y * 13 + x * y) % 3 === 0) cells.push({ x, y });
    }
  }
  return (
    <svg viewBox="0 0 25 25" className="size-28 shrink-0 rounded-md border border-pt-line bg-white p-1.5" role="img" aria-label="スマホで開くための二次元コード（見本）">
      {FINDER.map(({ fx, fy }) => (
        <g key={`${String(fx)}-${String(fy)}`}>
          <rect x={fx} y={fy} width="7" height="7" fill="#111" />
          <rect x={fx + 1} y={fy + 1} width="5" height="5" fill="#fff" />
          <rect x={fx + 2} y={fy + 2} width="3" height="3" fill="#111" />
        </g>
      ))}
      {cells.map((c) => <rect key={`${String(c.x)}-${String(c.y)}`} x={c.x} y={c.y} width="1" height="1" fill="#111" />)}
    </svg>
  );
}

export function Welcome() {
  const { go } = usePreviewNav();
  const [copied, setCopied] = useState(false);

  const copy = () => {
    void navigator.clipboard.writeText(PHONE_URL).then(
      () => { setCopied(true); },
      () => { setCopied(true); },
    );
  };

  return (
    <AuthLayout>
      <Heading title="登録が終わりました" lead="次回からは、メールアドレスとパスワードでログインできます。" />

      <div className="mb-5 hidden border-t border-pt-line pt-4 sm:block">
        <p className="text-[13px] font-medium text-pt-ink">スマホでも 1 度ログインしておく</p>
        <p className="mt-0.5 text-xs text-pt-muted">外出先で急ぐときに、すぐ開けます。</p>
        <div className="mt-3 flex items-center gap-4">
          <FakeQr />
          <div className="min-w-0 text-xs text-pt-muted">
            <p>スマホのカメラで読み取ります</p>
            <p className="pt-num mt-1 whitespace-nowrap text-[12px] text-pt-ink">{SHORT_URL}</p>
            <Button size="sm" variant="ghost" className="mt-1 -ml-2.5" onClick={copy}><IconCopy size={14} />{copied ? 'コピーしました' : 'アドレスをコピー'}</Button>
          </div>
        </div>
      </div>

      <p className="mb-5 text-sm leading-relaxed text-pt-muted sm:hidden">パソコンでも同じメールアドレスとパスワードで使えます。</p>

      <Button variant="primary" className={BTN_LG} onClick={() => { go(ROUTES.list); }}>応募者の一覧へ進む</Button>
    </AuthLayout>
  );
}
