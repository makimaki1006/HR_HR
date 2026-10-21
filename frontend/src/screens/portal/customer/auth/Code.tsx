import { useEffect, useState, type SyntheticEvent } from 'react';
import { log } from '../../data';
import { ROUTES, usePreviewNav } from '../../preview';
import { AuthLayout } from '../../ui/layouts';
import { Button, Field, Notice, cx } from '../../ui/kit';
import { BTN_LG, CheckRow, Fold, Heading, NETWORK_ERROR, TOUCH, useFakeSend } from './parts';

const WAIT_SECONDS = 60;

/** 6 つの枠の入力。貼り付けで全部入り、戻しキーで前の枠へ。 */
function CodeBoxes({ value, invalid, onChange }: { value: string; invalid: boolean; onChange: (v: string) => void }) {
  const focusAt = (i: number) => { document.getElementById(`code-${String(Math.min(Math.max(i, 0), 5))}`)?.focus(); };
  const setAt = (i: number, raw: string) => {
    // 1 マスには最後の 1 文字だけ入れる。複数桁は貼り付けでだけ受ける。
    const digits = raw.replace(/\D/g, '').slice(-1);
    if (digits === '') {
      onChange(value.slice(0, i) + value.slice(i + 1));
      return;
    }
    const next = (value.slice(0, i) + digits + value.slice(i + 1)).slice(0, 6);
    onChange(next);
    focusAt(i + digits.length);
  };
  return (
    <div className="flex justify-between gap-2" role="group" aria-label="6 桁のコード">
      {Array.from({ length: 6 }, (_, i) => (
        <input
          key={i}
          id={`code-${String(i)}`}
          value={value[i] ?? ''}
          inputMode="numeric"
          pattern="[0-9]*"
          autoComplete={i === 0 ? 'one-time-code' : 'off'}
          aria-label={`${String(i + 1)} 桁目`}
          aria-invalid={invalid}
          className={cx('pt-num h-12 w-full min-w-0 rounded-md border bg-pt-surface text-center text-xl font-medium text-pt-ink focus:border-pt-primary focus:outline-none focus:ring-2 focus:ring-pt-primary-soft', invalid ? 'border-pt-danger' : 'border-pt-line-strong')}
          onChange={(e) => { setAt(i, e.target.value); }}
          onFocus={(e) => { e.target.select(); }}
          onKeyDown={(e) => {
            if (e.key === 'Backspace' && !value[i]) focusAt(i - 1);
            if (e.key === 'ArrowLeft') focusAt(i - 1);
            if (e.key === 'ArrowRight') focusAt(i + 1);
          }}
          onPaste={(e) => {
            e.preventDefault();
            onChange(e.clipboardData.getData('text').replace(/\D/g, '').slice(0, 6));
            focusAt(5);
          }}
        />
      ))}
    </div>
  );
}
type Problem = 'wrong' | 'expired' | 'offline' | null;

export function Code() {
  const { go } = usePreviewNav();
  const [code, setCode] = useState('');
  const [remember, setRemember] = useState(false);
  const [problem, setProblem] = useState<Problem>(null);
  const [wait, setWait] = useState(WAIT_SECONDS);
  const [resent, setResent] = useState(false);
  const { sending, run } = useFakeSend();

  useEffect(() => {
    if (wait <= 0) return undefined;
    const t = window.setTimeout(() => { setWait((w) => w - 1); }, 1000);
    return () => { window.clearTimeout(t); };
  }, [wait]);

  const check = (c: string) => {
    if (c.length !== 6 || sending) return;
    run(() => {
      if (c === '111111') {
        setProblem('expired');
      } else if (c === '999999') {
        setProblem('offline');
      } else if (c === '000000') {
        setProblem('wrong');
        setCode('');
        log({ kind: 'login_failed', detail: '確認コードの誤り' });
        window.setTimeout(() => { document.getElementById('code-0')?.focus(); }, 0);
      } else {
        log({ kind: 'login', detail: remember ? '確認コードでログイン（この端末を 90 日覚える）' : '確認コードでログイン' });
        go(ROUTES.list);
      }
    });
  };

  const change = (raw: string) => {
    const c = raw.replace(/\D/g, '').slice(0, 6);
    setCode(c);
    setProblem(null);
    check(c);
  };

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    check(code);
  };

  const resend = () => {
    setWait(WAIT_SECONDS);
    setResent(true);
    setProblem(null);
    setCode('');
    log({ kind: 'code_sent', detail: '確認コードを送り直し' });
  };

  return (
    <AuthLayout>
      <Heading title="確認コード" lead="メールでお送りした 6 桁の数字を入力してください。" />
      <form onSubmit={submit} className="flex flex-col gap-4" noValidate>
        {problem === 'wrong' && <Notice tone="danger">コードが違います。もう一度確かめてください</Notice>}
        {problem === 'expired' && <Notice tone="warn">このコードは期限が切れました。下のボタンで送り直してください</Notice>}
        {problem === 'offline' && <Notice tone="warn">{NETWORK_ERROR}</Notice>}
        <Field label="6 桁のコード">
          <CodeBoxes value={code} invalid={problem === 'wrong'} onChange={change} />
        </Field>
        <CheckRow checked={remember} onChange={setRemember} label="この端末を覚える（90 日）" note="共用のパソコンでは使わないでください。" />
        <Button type="submit" variant="primary" className={BTN_LG} disabled={code.length !== 6 || sending}>{sending ? '確認しています…' : '確認する'}</Button>
      </form>
      <div className="mt-4 text-center text-sm">
        {resent && wait > 0 && <p className="mb-1 text-[13px] text-pt-ok">コードを送り直しました</p>}
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
      <div className="mt-2">
        <Fold summary="届かないとき">
          <p>送り先: t***@example.com（ログインのメール）、h***@example.com（追加の送り先）</p>
          <p className="mt-1">コードの有効時間は 10 分です。迷惑メールのフォルダもご確認ください。</p>
        </Fold>
      </div>
      <details className="mt-2 text-[11px] text-pt-faint">
        <summary className="cursor-pointer">試作の操作</summary>
        <p className="mt-1">000000: 違う／111111: 期限切れ／999999: 通信できない／それ以外: 一覧へ</p>
      </details>
    </AuthLayout>
  );
}
