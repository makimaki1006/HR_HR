// ① 事実抽出 (旧 renderExtract)。
import { ConfirmBox, GateBadge, SectionHead } from '../parts';
import { type Facts, FKEY_JA, type StepKey } from '../state';

const ST_JA: Record<string, string> = {
  verified: '検証済',
  rejected: '要確認',
  missing: '未取得',
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
            detail={`確認済み ${String(nv)}／要確認 ${String(nr)}／未取得 ${String(nm)}`}
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
                    <td className="fkey">{FKEY_JA[k] ?? 'その他の項目'}</td>
                    <td>{status === 'verified' && f?.value ? f.value : '未取得'}</td>
                    <td className="fquote">{f?.evidence_quote ? `「${f.evidence_quote}」` : '—'}</td>
                    <td>
                      <span className={`fst ${status}`}>{ST_JA[status] ?? '不明'}</span>
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
        元の資料で確かめられない値は採用していません。「要確認」の項目は引用と元の資料を照らし合わせてください。「未取得」は資料から確認できなかった項目です。
      </div>
      <ConfirmBox stepKey="extract" checked={confirmed} onChange={onConfirm} />
    </>
  );
}
