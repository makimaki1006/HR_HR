import { useEffect, useState, type SyntheticEvent } from 'react';
import { log } from '../../data';
import { ROUTES, usePreviewNav } from '../../preview';
import { AuthLayout } from '../../ui/layouts';
import { Button, Field, Notice } from '../../ui/kit';
import { CodeBoxes, toDigits } from './CodeBoxes';
import { BTN_LG, CheckRow, Fold, Heading, NETWORK_ERROR, TOUCH, useFakeSend } from './parts';

const WAIT_SECONDS = 60;

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
    const c = toDigits(raw).slice(0, 6);
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
        <CheckRow checked={remember} onChange={setRemember} label="この端末を覚える" note="90 日。自分だけが使う端末のとき" />
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
      <details className="mt-2 text-[12px] text-pt-faint">
        <summary className="cursor-pointer">試作の操作</summary>
        <p className="mt-1">000000: 違う／111111: 期限切れ／999999: 通信できない／それ以外: 一覧へ</p>
      </details>
    </AuthLayout>
  );
}
