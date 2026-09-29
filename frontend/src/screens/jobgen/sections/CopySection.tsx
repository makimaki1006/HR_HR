// ④ キャッチコピー (旧 renderCopy)。
import {
  ConfirmBox,
  ExprWarnings,
  GateBadge,
  NgViolations,
  NumberCheckNote,
  NumberViolations,
  SectionHead,
} from '../parts';
import type { CopyResult, StepKey } from '../state';

export function CopySection({
  copies,
  confirmed,
  onConfirm,
}: {
  copies: CopyResult[];
  confirmed: boolean;
  onConfirm: (key: StepKey, checked: boolean) => void;
}) {
  const totalNg = copies.reduce((n, c) => n + c.ng_violations.length, 0);
  const totalNv = copies.reduce((n, c) => n + c.number_violations.length, 0);
  const totalEw = copies.reduce((n, c) => n + c.expression_warnings.length, 0);
  return (
    <>
      <SectionHead
        num="④"
        name="キャッチコピー"
        gates={
          <>
            <GateBadge
              label="法令NGワード"
              cls={totalNg ? 'bad' : 'ok'}
              detail={totalNg ? `違反 ${String(totalNg)}件` : '通過'}
            />
            <GateBadge
              label="数値照合"
              cls={totalNv ? 'bad' : 'ok'}
              detail={totalNv ? `原文にない数値 ${String(totalNv)}件` : '通過'}
            />
            {totalEw ? (
              <GateBadge label="表現レビュー" cls="warn" detail={`要確認 ${String(totalEw)}件`} />
            ) : null}
          </>
        }
      />
      {copies.map((c, i) => {
        if (c.error !== undefined) {
          return (
            <div key={i} className="copyblk">
              <div className="cplabel">{c.label}</div>
              <div className="err">{c.error}</div>
            </div>
          );
        }
        const warn =
          c.review_required || c.number_violations.length || c.expression_warnings.length;
        return (
          <div key={i} className="copyblk">
            <div className="cplabel">
              {c.label}
              {warn ? (
                <>
                  {' '}
                  <span className="gstat review">要確認</span>
                </>
              ) : null}
            </div>
            {c.copies.map((cp, j) => (
              <div key={j} className="chip">
                <span className="cstyle">{cp.style || ''}</span>
                {cp.text || ''}
              </div>
            ))}
            <NgViolations items={c.ng_violations} />
            <NumberViolations items={c.number_violations} />
            <ExprWarnings items={c.expression_warnings} />
            <NumberCheckNote check={c.number_check} />
          </div>
        );
      })}
      <ConfirmBox stepKey="copy" checked={confirmed} onChange={onConfirm} />
    </>
  );
}
