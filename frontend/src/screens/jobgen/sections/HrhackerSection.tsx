// ⑦ 84 列原稿＋数値照合 (旧 renderHrhacker)。
import { useState } from 'react';
import { downloadHrhackerCsv, HRHACKER_COLUMNS } from '../csv';
import { fieldLabel, fieldValue, reviewIssue } from '../presentation';
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
  const cols = HRHACKER_COLUMNS;
  const [filter, setFilter] = useState('all');
  const [search, setSearch] = useState('');
  const genEntries = Object.entries(h.generated_fields).flatMap(([k, g]) =>
    g ? [[k, g] as const] : [],
  );
  const genReview = genEntries.some(([, g]) => g.status === 'review_required');
  const numberIssues = h.unsupported_numbers.filter(issue => issue.startsWith('unsupported_numbers:'));
  const genKeys = new Set(genEntries.map(([k, g]) => g.column || k));
  const fs = h.fill_stats;
  const uh = h.unassigned_hints;
  const needsReview = (c: string): boolean => h.review_required_fields.includes(c) || genEntries.some(([k, g]) => (g.column || k) === c && g.status !== 'generated_verified');
  const visibleCols = cols.filter(c => fieldLabel(c).includes(search.trim()) && (
    filter === 'all' || filter === 'filled' && !!row[c]?.trim() || filter === 'missing' && !row[c]?.trim() || filter === 'review' && needsReview(c)
  ));
  return (
    <>
      <SectionHead
        num="⑦"
        name="84列原稿＋数値照合"
        gates={
          <>
            <GateBadge
              label="数値照合"
              cls={numberIssues.length ? 'bad' : 'ok'}
              detail={
                numberIssues.length
                  ? `未照合 ${String(numberIssues.length)}件`
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
      {numberIssues.length ? (
        <div className="badnums">
          <b>元の資料で確認できない数値があります。</b> 該当する生成項目を確認してください。
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
                  <th>確認する項目</th>
                  <th>原文の該当箇所</th>
                </tr>
              </thead>
              <tbody>
                {uh.map((u, i) => (
                  <tr key={i}>
                    <td className="colcell">{fieldLabel(u.column || '')}</td>
                    <td className="fquote">{u.evidence || ''}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : null}
      <h3 style={H3_STYLE}>作成した文章（数値・文字数・表現を確認）</h3>
      <div className="genwrap">
        {genEntries.length ? (
          genEntries.map(([k, g]) => {
            const ok = g.status === 'generated_verified';
            const cls = ok ? 'verified' : 'review';
            return (
              <div key={k} className={`gencard ${cls}`}>
                <div className="gcol">
                  {fieldLabel(g.column || k)} <span className={`gstat ${cls}`}>{ok ? '検証済' : '要確認'}</span>
                </div>
                <div className={`gval${g.value ? '' : ' empty'}`}>
                  {g.value ? g.value : '（空欄・レビュー行き）'}
                </div>
                {g.issues.length ? (
                  <div className="gissues">
                    課題:
                    <ul>
                      {g.issues.map((x, i) => (
                        <li key={i}>{reviewIssue(x)}</li>
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
          Excelで開ける形式で、求人1件を保存します。確認を通らなかった文章は空欄で出力します。
        </span>
      </div>
      <h3 style={H3_STYLE}>出力内容の確認（{cols.length}項目）</h3>
      <div className="ctl">
        <label htmlFor="reviewFilter">表示する項目</label>
        <select id="reviewFilter" value={filter} onChange={e => { setFilter(e.currentTarget.value); }}>
          <option value="all">すべて</option>
          <option value="filled">入力済み</option>
          <option value="review">要確認</option>
          <option value="missing">未取得</option>
        </select>
        <label htmlFor="reviewSearch">項目を探す</label>
        <input id="reviewSearch" type="search" placeholder="例：給与、休日" value={search} onChange={e => { setSearch(e.currentTarget.value); }} />
        <span className="hint">表示中 {visibleCols.length}項目 · CSVは84列すべて出力します。</span>
      </div>
      <div className="tblwrap">
        <table id="reviewTable">
          <thead>
            <tr>
              <th>項目</th>
              <th>値</th>
              <th>区分</th>
            </tr>
          </thead>
          <tbody>
            {visibleCols.map((c) => {
              const isGen = genKeys.has(c);
              return (
                <tr key={c} className={isGen ? 'row-gen' : ''}>
                  <td className="colcell">{fieldLabel(c)}</td>
                  <td>
                    <div className="valwrap">{fieldValue(c, row[c])}</div>
                  </td>
                  <td className="colcell">{isGen ? needsReview(c) ? '要確認' : '生成（検証済）' : row[c] ? '元の資料から転記' : '未取得'}</td>
                </tr>
              );
            })}
            {!visibleCols.length ? <tr><td colSpan={3}>該当する項目はありません。表示条件を変えてください。</td></tr> : null}
          </tbody>
        </table>
      </div>
      <ConfirmBox stepKey="hrhacker" checked={confirmed} onChange={onConfirm} />
    </>
  );
}
