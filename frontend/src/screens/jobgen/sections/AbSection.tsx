// ⑧ A/B テスト助言 (旧 renderAb)。
import {
  ConfirmBox,
  ExprWarnings,
  GateBadge,
  NgViolations,
  NumberCheckNote,
  NumberViolations,
  SectionHead,
} from '../parts';
import type { AbResult, StepKey } from '../state';

export function AbSection({
  ab,
  confirmed,
  onConfirm,
}: {
  ab: AbResult;
  confirmed: boolean;
  onConfirm: (key: StepKey, checked: boolean) => void;
}) {
  const ng = ab.ng_violations;
  const nv = ab.number_violations;
  const ew = ab.expression_warnings;
  return (
    <>
      <SectionHead
        num="⑧"
        name="A/Bテスト助言"
        gates={
          <>
            <GateBadge
              label="法令NGワード"
              cls={ng.length ? 'bad' : 'ok'}
              detail={ng.length ? `違反 ${String(ng.length)}件` : '通過'}
            />
            <GateBadge
              label="数値照合"
              cls={nv.length ? 'bad' : 'ok'}
              detail={nv.length ? `原文にない数値 ${String(nv.length)}件` : '通過'}
            />
            {ew.length ? (
              <GateBadge label="表現レビュー" cls="warn" detail={`要確認 ${String(ew.length)}件`} />
            ) : null}
          </>
        }
      />
      <NgViolations items={ng} />
      <NumberViolations items={nv} />
      <ExprWarnings items={ew} />
      <NumberCheckNote check={ab.number_check} />
      {ab.steps.length ? (
        ab.steps.map((s, i) => (
          <div key={i} className="abrow">
            <div className="abm">{s.metric || ''}</div>
            <div>{s.action || ''}</div>
          </div>
        ))
      ) : (
        <div className="note">助言がありません。</div>
      )}
      <ConfirmBox stepKey="ab" checked={confirmed} onChange={onConfirm} />
    </>
  );
}
