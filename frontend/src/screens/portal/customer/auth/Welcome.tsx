import { useState } from 'react';
import { ROUTES, usePreviewNav } from '../../preview';
import { AuthLayout } from '../../ui/layouts';
import { Button, Notice } from '../../ui/kit';
import { Heading } from './parts';

import { LOGIN_URL } from '../../fixtures';

const PHONE_URL = LOGIN_URL;
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
    <svg viewBox="0 0 25 25" className="size-40 rounded-md border border-pt-line bg-white p-2" role="img" aria-label="スマホで開くための二次元コード（見本）">
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
      <Heading title="登録が終わりました" lead="パスワードを設定しました。次回からは、メールアドレスとパスワードでログインできます。この端末では、次回からコードなしで開けます。" />

      <div className="mb-5 hidden sm:block">
        <Notice tone="info" title="スマホでも、今のうちに 1 回ログインしてください">
          外出先で急ぎで見たいときに、初めてのログインだと確認コードが要ります。あらかじめ一度ログインしておくと、そのスマホではすぐ開けます。
        </Notice>
        <div className="mt-4 flex flex-col items-center gap-2">
          <FakeQr />
          <p className="text-sm text-pt-muted">スマホのカメラで読み取ると開きます</p>
          <div className="flex items-center gap-2 text-xs text-pt-muted">
            <span className="break-all">{PHONE_URL}</span>
            <Button size="sm" variant="ghost" onClick={copy}>{copied ? 'コピーしました' : 'アドレスをコピー'}</Button>
          </div>
        </div>
      </div>

      <p className="mb-5 text-sm leading-relaxed text-pt-muted sm:hidden">パソコンでも同じメールアドレスとパスワードで使えます。</p>

      <Button variant="primary" size="lg" className="w-full" onClick={() => { go(ROUTES.list); }}>応募者の一覧へ進む</Button>
    </AuthLayout>
  );
}
