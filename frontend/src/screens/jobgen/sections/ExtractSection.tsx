// ① 事実抽出 (旧 renderExtract)。
import { ConfirmBox, GateBadge, SectionHead } from '../parts';
import { type Facts, FKEY_JA, type StepKey } from '../state';

const ST_JA: Record<string, string> = {
  verified: '検証済',
  rejected: 'リジェクト',
  missing: '欠落',
};

export function ExtractSection({
  facts,
  confirmed,
  onConfirm,
}: {
  facts: Facts;
  confirmed: boolean;
  onConfirm: (key: StepKey, checked: boolean) => void;
}) {
  const keys = Object.keys(facts);
  const count = (st: string) => keys.filter((k) => facts[k]?.status === st).length;
  const nv = count('verified');
  const nr = count('rejected');
  const nm = count('missing');
  return (
    <>
      <SectionHead
        num="①"
        name="事実抽出"
        gates={
          <GateBadge
            label="引用照合"
            cls={nr ? 'bad' : 'ok'}
            detail={`検証 ${String(nv)}／リジェクト ${String(nr)}／欠落 ${String(nm)}`}
          />
        }
      />
      <div className="tblwrap">
        <table>
          <thead>
            <tr>
              <th>項目</th>
              <th>値</th>
              <th>原文の引用（一字一句照合）</th>
              <th>状態</th>
            </tr>
          </thead>
          <tbody>
            {keys.length ? (
              keys.map((k) => {
                const f = facts[k];
                const status = f?.status ?? '';
                const cls =
                  status === 'rejected' ? 'row-rejected' : status === 'missing' ? 'row-missing' : '';
                return (
                  <tr key={k} className={cls}>
                    <td className="fkey">{FKEY_JA[k] ?? k}</td>
                    <td>{f?.value ?? ''}</td>
                    <td className="fquote">{f?.evidence_quote ? `「${f.evidence_quote}」` : '—'}</td>
                    <td>
                      <span className={`fst ${status}`}>{ST_JA[status] ?? status}</span>
                    </td>
                  </tr>
                );
              })
            ) : (
              <tr>
                <td colSpan={4}>抽出項目がありません。</td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
      <div className="note">
        引用が原文に文字列一致しない項目は自動リジェクト（赤）→空欄＋レビュー。原文に無い項目は欠落（グレー）。
      </div>
      <ConfirmBox stepKey="extract" checked={confirmed} onChange={onConfirm} />
    </>
  );
}
