// ③ ペルソナ設計 (旧 renderPersonas)。
import type { Persona } from '../../../generated/Persona';
import { ConfirmBox, SectionHead } from '../parts';
import type { StepKey } from '../state';

export function PersonasSection({
  personas,
  confirmed,
  onConfirm,
}: {
  personas: Persona[];
  confirmed: boolean;
  onConfirm: (key: StepKey, checked: boolean) => void;
}) {
  return (
    <>
      <SectionHead num="③" name="ペルソナ設計" />
      <div className="hypo-note">
        以下は生成した<b>仮説ペルソナ</b>
        です。年齢・家族構成・生活環境などは求人原文から確認された事実ではなく、狙う応募者像を想定して作った仮説です。実在人物の情報として扱わないでください。
      </div>
      <div className="pgrid">
        {personas.length ? (
          personas.map((p, i) => (
            <div key={i} className="pcard">
              <div className="plabel">
                {p.label || 'ペルソナ' + String(i + 1)}
                <span className="pbadge">仮説</span>
              </div>
              <div className="pprofile">{p.profile || ''}</div>
              <div className="pfield">
                <span className="pk">現職の不満</span>
                {p.dissatisfaction || '—'}
              </div>
              <div className="pfield">
                <span className="pk">生活環境</span>
                {p.environment || '—'}
              </div>
              <div className="pfield">
                <span className="pk">痛み</span>
                {p.pain || '—'}
              </div>
            </div>
          ))
        ) : (
          <div className="note">ペルソナがありません。</div>
        )}
      </div>
      <ConfirmBox stepKey="personas" checked={confirmed} onChange={onConfirm} />
    </>
  );
}
