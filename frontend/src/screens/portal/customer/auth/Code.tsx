import { useEffect, useState, type SyntheticEvent } from 'react';
import { ROUTES, usePreviewNav } from '../../preview';
import { AuthLayout } from '../../ui/layouts';
import { Button, Field, Notice, TextInput, cx } from '../../ui/kit';
import { Heading, NETWORK_ERROR, TOUCH, useFakeSend } from './parts';

const WAIT_SECONDS = 60;
type Problem = 'wrong' | 'expired' | 'offline' | null;

export function Code() {
  const { go } = usePreviewNav();
  const [code, setCode] = useState('');
  const [remember, setRemember] = useState(true);
  const [problem, setProblem] = useState<Problem>(null);
  const [wait, setWait] = useState(WAIT_SECONDS);
  const [resent, setResent] = useState(false);
  const { sending, run } = useFakeSend();

  useEffect(() => {
    if (wait <= 0) return undefined;
    const t = window.setTimeout(() => { setWait((w) => w - 1); }, 1000);
    return () => { window.clearTimeout(t); };
  }, [wait]);

  const change = (raw: string) => {
    setCode(raw.replace(/\D/g, '').slice(0, 6));
    setProblem(null);
  };

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    if (code.length !== 6 || sending) return;
    run(() => {
      if (code === '111111') setProblem('expired');
      else if (code === '999999') setProblem('offline');
      else if (code === '000000') {
        setProblem('wrong');
        window.setTimeout(() => { document.getElementById('code-input')?.focus(); (document.getElementById('code-input') as HTMLInputElement | null)?.select(); }, 0);
      } else go(ROUTES.list);
    });
  };

  const resend = () => { setWait(WAIT_SECONDS); setResent(true); setProblem(null); setCode(''); };

  return (
    <AuthLayout>
      <Heading title="確認コード" lead="この端末でのログインは初めてです。メールでお送りした 6 桁の数字を入力してください。" />
      <form onSubmit={submit} className="flex flex-col gap-4" noValidate>
        {problem === 'wrong' && <Notice tone="danger">コードが違います。もう一度確かめてください</Notice>}
        {problem === 'expired' && <Notice tone="warn">このコードは期限が切れました。下のボタンで送り直してください</Notice>}
        {problem === 'offline' && <Notice tone="warn">{NETWORK_ERROR}</Notice>}
        <Field label="6 桁のコード" htmlFor="code-input">
          <TextInput
            id="code-input"
            name="code"
            value={code}
            inputMode="numeric"
            pattern="[0-9]*"
            autoComplete="one-time-code"
            maxLength={6}
            aria-invalid={problem === 'wrong'}
            placeholder="6 桁の数字"
            className={cx('h-14 text-center text-2xl tracking-[0.5em] placeholder:text-sm placeholder:tracking-normal', problem === 'wrong' && 'border-pt-danger')}
            onChange={(e) => { change(e.target.value); }}
            onPaste={(e) => {
              e.preventDefault();
              change(e.clipboardData.getData('text'));
            }}
          />
        </Field>
        <label className="flex items-start gap-2 text-sm">
          <input type="checkbox" checked={remember} className="mt-0.5 size-4" onChange={(e) => { setRemember(e.target.checked); }} />
          <span>
            この端末を覚える（90 日）
            <span className="block text-xs text-pt-muted">最後に使った日から 90 日、コードの入力が要りません。共用のパソコンでは外してください。</span>
          </span>
        </label>
        <Button type="submit" variant="primary" size="lg" disabled={code.length !== 6 || sending}>{sending ? '確認しています…' : '確認する'}</Button>
      </form>
      <div className="mt-4 text-center text-sm">
        {resent && wait > 0 && <p className="mb-1 text-pt-ok">コードを送り直しました</p>}
        <Button
          variant={problem === 'expired' ? 'primary' : 'ghost'}
          size="sm"
          className={TOUCH}
          disabled={wait > 0}
          onClick={resend}
        >
          {wait > 0 ? `コードを送り直す（あと ${String(wait)} 秒）` : 'コードを送り直す'}
        </Button>
        <div>
          <Button variant="ghost" size="sm" className={TOUCH} onClick={() => { go(ROUTES.login); }}>ログインに戻る</Button>
        </div>
      </div>
      <details className="mt-3 rounded-md border border-pt-line bg-pt-bg px-4 py-2 text-sm">
        <summary className="cursor-pointer text-pt-muted">届かないとき</summary>
        <div className="mt-2">
          <p className="text-pt-muted">送り先</p>
          <ul className="mt-1 space-y-0.5">
            <li>t***@example.com（ログインのメールアドレス）</li>
            <li>h***@example.com（追加の送り先）</li>
          </ul>
          <p className="mt-2 text-xs text-pt-muted">メールの件名にコードが入っているので、通知の表示だけでも読めます。有効な時間は 10 分です。迷惑メールのフォルダもご確認ください。</p>
        </div>
      </details>
      <details className="mt-3 border-t border-pt-line pt-3 text-xs text-pt-faint">
        <summary className="cursor-pointer">試作の操作</summary>
        <p className="mt-1">000000: 違う／111111: 期限切れ／999999: 通信できない／それ以外: 一覧へ</p>
      </details>
    </AuthLayout>
  );
}
