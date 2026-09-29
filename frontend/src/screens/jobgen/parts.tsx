// 結果セクション共通の小部品 (旧 static/jobgen.html の gateBadge / sectionHead /
// ngViolationsHtml / numberViolationsHtml / exprWarningsHtml / numberCheckNote / confirmBox /
// copyText / stalebar)。文言は旧と同じ。
import { type ReactNode, useEffect, useRef, useState } from 'react';
import type { NgViolation } from '../../generated/NgViolation';
import type { NumberViolation } from '../../generated/NumberViolation';
import { type StepKey, stepDef } from './state';

export function GateBadge({ label, cls, detail }: { label: string; cls: string; detail?: string }) {
  return (
    <span className={`gate ${cls}`}>
      {label}
      {detail ? `：${detail}` : ''}
    </span>
  );
}

export function SectionHead({
  num,
  name,
  gates,
}: {
  num: string;
  name: string;
  gates?: ReactNode;
}) {
  return (
    <>
      <h2>
        {num} {name}
      </h2>
      <div className="gates">{gates}</div>
    </>
  );
}

function violationLabel(v: NgViolation): string {
  const scope = v.major || v.minor ? `（${v.major || ''}${v.minor ? '×' + v.minor : ''}）` : '';
  return `${v.matched || ''}${scope}`;
}

/** 法令NGワード違反ブロック (④⑥⑧共通)。 */
export function NgViolations({ items }: { items: NgViolation[] }) {
  if (!items.length) return null;
  return (
    <div className="ngbadge">
      <b>⚠ 法令NGワード違反 {items.length}件</b>
      <ul>
        {items.map((v, i) => (
          <li key={i}>
            {violationLabel(v)}: {v.reason || ''}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** 原文にない数値の警告ブロック (④⑤⑥⑧共通)。 */
export function NumberViolations({ items }: { items: NumberViolation[] }) {
  if (!items.length) return null;
  return (
    <div className="numviol">
      <b>⚠ 原文にない数値 {items.length}件</b>
      <div className="nvhint">
        原文で確認できない数値が使われています。事実か確認し、根拠がなければ修正してください。
      </div>
      <ul>
        {items.map((v, i) => (
          <li key={i}>
            {v.text || ''}
            {v.numbers.length ? (
              <>
                {'　'}→ 数値: <b>{v.numbers.join(' / ')}</b>
              </>
            ) : null}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** 表現レビュー (法令違反ではないが顧客提示前に見直し推奨)。 */
export function ExprWarnings({ items }: { items: NgViolation[] }) {
  if (!items.length) return null;
  return (
    <div className="exprwarn">
      <b>△ 表現レビュー（警告）{items.length}件</b>
      <div className="ewhint">
        法令違反ではありませんが、顧客に提示する前に見直しをおすすめします。
      </div>
      <ul>
        {items.map((v, i) => (
          <li key={i}>
            {violationLabel(v)}
            {v.severity ? ` [${v.severity}]` : ''}: {v.reason || ''}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** 数値照合が未実施だった場合の注記 (通常は source_text 送信で出ない)。 */
export function NumberCheckNote({ check }: { check: string }) {
  if (!check || check === 'checked') return null;
  return <div className="ncnote">数値照合: 未実施（原文未提供）</div>;
}

/** 各工程結果セクション末尾の「コンサル確認済みにする」チェック。 */
export function ConfirmBox({
  stepKey,
  checked,
  onChange,
}: {
  stepKey: StepKey;
  checked: boolean;
  onChange: (key: StepKey, checked: boolean) => void;
}) {
  return (
    <label className={`confirmbox${checked ? ' on' : ''}`}>
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => {
          onChange(stepKey, e.currentTarget.checked);
        }}
      />
      この工程の生成物を目視で確認しました（コンサル確認済みにする）
    </label>
  );
}

/** 前工程が更新されたことを示す帯。 */
export function StaleBar({ source }: { source: StepKey }) {
  const { num, name } = stepDef(source);
  return (
    <div className="stalebar">
      ⚠ {num} {name}{' '}
      を実行し直したため、この結果は古い可能性があります。必要なら再実行してください。
    </div>
  );
}

/** 工程が失敗したときのセクション本体。 */
export function FailBody({ stepKey, message }: { stepKey: StepKey; message: string }) {
  const { num, name } = stepDef(stepKey);
  return (
    <>
      <SectionHead num={num} name={name} gates={<GateBadge label="失敗" cls="bad" />} />
      <div className="err">{message}</div>
    </>
  );
}

/** クリップボードへコピー。1.5 秒だけ「✓ コピーしました」にする。 */
export function CopyButton({ text, label }: { text: string; label: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current !== null) clearTimeout(timer.current);
    },
    [],
  );
  return (
    <button
      type="button"
      className="copybtn"
      disabled={copied}
      onClick={() => {
        navigator.clipboard.writeText(text).then(
          () => {
            setCopied(true);
            timer.current = setTimeout(() => {
              setCopied(false);
            }, 1500);
          },
          () => {
            alert('コピーに失敗しました。テキストを選択して手動でコピーしてください。');
          },
        );
      }}
    >
      {copied ? '✓ コピーしました' : label}
    </button>
  );
}

/** 原稿行 (空行は高さだけ確保)。 */
export function ManuscriptLines({ lines }: { lines: string[] }) {
  if (!lines.length) return <div className="note">原稿なし</div>;
  return (
    <>
      {lines.map((l, i) =>
        l === '' ? <div key={i} className="ml-empty" /> : <div key={i} className="ml-line">{l}</div>,
      )}
    </>
  );
}
