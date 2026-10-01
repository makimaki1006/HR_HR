// ⑦ 84 列原稿＋数値照合 (旧 renderHrhacker)。
import { downloadHrhackerCsv } from '../csv';
import { ConfirmBox, GateBadge, SectionHead } from '../parts';
import type { HrhackerResult, StepKey } from '../state';

const H3_STYLE = { fontSize: '13px', margin: '6px 0 8px' } as const;

export function HrhackerSection({
  h,
  confirmed,
  onConfirm,
}: {
  h: HrhackerResult;
  confirmed: boolean;
  onConfirm: (key: StepKey, checked: boolean) => void;
}) {
  const row = h.row;
  const cols = Object.keys(row);
  const genEntries = Object.entries(h.generated_fields).flatMap(([k, g]) =>
    g ? [[k, g] as const] : [],
  );
  const genReview = genEntries.some(([, g]) => g.status === 'review_required');
  const genKeys = new Set(genEntries.map(([k, g]) => g.column || k));
  const fs = h.fill_stats;
  const uh = h.unassigned_hints;
  return (
    <>
      <SectionHead
        num="⑦"
        name="84列原稿＋数値照合"
        gates={
          <>
            <GateBadge
              label="数値照合[E]"
              cls={h.unsupported_numbers.length ? 'bad' : 'ok'}
              detail={
                h.unsupported_numbers.length
                  ? `未照合 ${String(h.unsupported_numbers.length)}件`
                  : '通過'
              }
            />
            <GateBadge
              label="文字数・NGワード"
              cls={genReview ? 'warn' : 'ok'}
              detail={genReview ? 'レビュー要あり' : '通過'}
            />
            <GateBadge
              label="生成列"
              cls={h.review_required_fields.length ? 'warn' : 'ok'}
              detail={`要レビュー ${String(h.review_required_fields.length)}列`}
            />
          </>
        }
      />
      {fs ? (
        <div className="fillstat">
          転記充足:{' '}
          <b>
            {fs.filled}/{fs.total}
          </b>{' '}
          列（原文に対応する列{' '}
          <b>
            {fs.fact_mapped_filled}/{fs.fact_mapped_total}
          </b>
          ）
        </div>
      ) : null}
      {h.unsupported_numbers.length ? (
        <div className="badnums">
          <b>原文に無い数値（リジェクト）:</b> {h.unsupported_numbers.join(' / ')}
        </div>
      ) : null}
      {uh.length ? (
        <div className="unassigned">
          <div className="uhhead">
            📋 原文に記載があるのに未転記の可能性がある項目（{uh.length}件）
          </div>
          <div className="tblwrap">
            <table>
              <thead>
                <tr>
                  <th>列名（候補）</th>
                  <th>原文の該当箇所</th>
                </tr>
              </thead>
              <tbody>
                {uh.map((u, i) => (
                  <tr key={i}>
                    <td className="colcell">{u.column || ''}</td>
                    <td className="fquote">{u.evidence || ''}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : null}
      <h3 style={H3_STYLE}>生成5列（数値照合＋文字数＋NGワード）</h3>
      <div className="genwrap">
        {genEntries.length ? (
          genEntries.map(([k, g]) => {
            const ok = g.status === 'generated_verified';
            const cls = ok ? 'verified' : 'review';
            return (
              <div key={k} className={`gencard ${cls}`}>
                <div className="gcol">
                  {g.column || k} <span className={`gstat ${cls}`}>{ok ? '検証済' : 'レビュー要'}</span>
                </div>
                <div className={`gval${g.value ? '' : ' empty'}`}>
                  {g.value ? g.value : '（空欄・レビュー行き）'}
                </div>
                {g.issues.length ? (
                  <div className="gissues">
                    課題:
                    <ul>
                      {g.issues.map((x, i) => (
                        <li key={i}>{x}</li>
                      ))}
                    </ul>
                  </div>
                ) : null}
              </div>
            );
          })
        ) : (
          <div className="note">生成列なし</div>
        )}
      </div>
      <div className="csvbar">
        <button
          type="button"
          className="btn"
          id="csvBtn"
          onClick={() => {
            downloadHrhackerCsv(row);
          }}
        >
          84列CSVをダウンロード
        </button>
        <span className="hint">
          UTF-8 BOM付き・ヘッダ1行＋データ1行。検証を通らなかった生成列は空欄です。
        </span>
      </div>
      <h3 style={H3_STYLE}>84列 確認テーブル（{cols.length}列）</h3>
      <div className="tblwrap">
        <table>
          <thead>
            <tr>
              <th>列名</th>
              <th>値</th>
              <th>区分</th>
            </tr>
          </thead>
          <tbody>
            {cols.map((c) => {
              const isGen = genKeys.has(c);
              return (
                <tr key={c} className={isGen ? 'row-gen' : ''}>
                  <td className="colcell">{c}</td>
                  <td>
                    <div className="valwrap">{row[c]}</div>
                  </td>
                  <td className="colcell">{isGen ? '生成（検証済）' : '不変転記／スロット'}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <ConfirmBox stepKey="hrhacker" checked={confirmed} onChange={onConfirm} />
    </>
  );
}
