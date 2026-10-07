import { useId, useState } from 'react';
import type { ChangeEvent } from 'react';
import type { JobCopyRecord } from './data';
import type { BillingPeriod, BillingTaxBasis } from './billingTypes';
import {
  BILLING_FIELDS, BillingCsvError, MAX_BILLING_FILE_BYTES, billingMappingProblems, buildBillingImport,
  decodeBillingCsv, guessBillingColumns, guessTaxBasis, parseCsv,
} from './billingImport';
import type { BillingColumnMapping, BillingEncoding, BillingImportResult, BillingRowIssue } from './billingImport';
import './billing-import.css';

type EncodingChoice = BillingEncoding | 'auto';
const encodingLabels: Record<EncodingChoice, string> = { auto: '自動で判定', 'utf-8': 'UTF-8', shift_jis: 'Excel で保存した日本語の CSV' };
const yen = (value: number) => `${Math.round(value).toLocaleString('ja-JP')}円`;

function IssueList({ title, issues }: { title: string; issues: readonly BillingRowIssue[] }) {
  if (!issues.length) return null;
  return <details className="jc-billing-issues"><summary>{title}（{String(issues.length)}行）</summary>
    <ul>{issues.slice(0, 200).map(issue => <li key={`${String(issue.row)}-${issue.message}`}><strong>{String(issue.row)}行目</strong> {issue.message}</li>)}</ul>
    {issues.length > 200 && <p className="jc-muted">先頭の200行だけを表示しています。</p>}
  </details>;
}

/**
 * 課金CSVを 4 段階 (ファイルを選ぶ → 列の確認 → 求人との照合 → 反映) で取り込む。
 * 反映した課金はこの画面のメモリ上だけで持ち、サーバーへは送らない。
 */
export function BillingImportPanel({ records, applied, onApply, onClear }: {
  records: readonly JobCopyRecord[];
  applied: readonly BillingPeriod[];
  onApply: (periods: BillingPeriod[]) => void;
  onClear: () => void;
}) {
  const id = useId();
  const [encoding, setEncoding] = useState<EncodingChoice>('auto');
  const [bytes, setBytes] = useState<ArrayBuffer | null>(null);
  const [fileName, setFileName] = useState('');
  const [rows, setRows] = useState<string[][] | null>(null);
  const [usedEncoding, setUsedEncoding] = useState<BillingEncoding | null>(null);
  const [mapping, setMapping] = useState<BillingColumnMapping>({});
  const [taxBasis, setTaxBasis] = useState<BillingTaxBasis>('不明');
  const [result, setResult] = useState<BillingImportResult | null>(null);
  const [error, setError] = useState('');
  const [reading, setReading] = useState(false);
  const [appliedNow, setAppliedNow] = useState(false);

  function load(source: ArrayBuffer, choice: EncodingChoice) {
    setRows(null); setResult(null); setError(''); setAppliedNow(false); setUsedEncoding(null);
    try {
      const decoded = decodeBillingCsv(source, choice);
      const parsed = parseCsv(decoded.text);
      const headers = parsed[0];
      if (!headers || parsed.length < 2) throw new BillingCsvError('見出し行と 1 行以上のデータがある CSV を選んでください。');
      const guessed = guessBillingColumns(headers);
      setRows(parsed); setUsedEncoding(decoded.encoding); setMapping(guessed);
      setTaxBasis(guessTaxBasis(guessed.amount === undefined ? undefined : headers[guessed.amount]));
    } catch (caught) {
      setError(caught instanceof BillingCsvError ? caught.message : 'CSV を読み込めませんでした。ファイルの形を確認してください。');
    }
  }

  async function choose(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;
    setBytes(null); setFileName(''); setRows(null); setResult(null); setError(''); setAppliedNow(false);
    if (file.size > MAX_BILLING_FILE_BYTES) { setError('5MB 以下の CSV を選んでください。'); return; }
    setReading(true);
    try {
      const buffer = await file.arrayBuffer();
      setBytes(buffer); setFileName(file.name);
      load(buffer, encoding);
    } catch {
      setError('ファイルを読み込めませんでした。');
    } finally { setReading(false); }
  }

  function changeEncoding(value: EncodingChoice) {
    setEncoding(value);
    if (bytes) load(bytes, value);
  }

  function changeColumn(field: keyof BillingColumnMapping, value: string) {
    setResult(null); setAppliedNow(false);
    setMapping(current => {
      const next: BillingColumnMapping = {};
      for (const spec of BILLING_FIELDS) {
        const column = spec.field === field ? (value === '' ? undefined : Number(value)) : current[spec.field];
        if (column !== undefined) next[spec.field] = column;
      }
      return next;
    });
  }

  function check() {
    if (!rows) return;
    setAppliedNow(false);
    try { setResult(buildBillingImport(rows, mapping, records, taxBasis)); setError(''); }
    catch (caught) { setResult(null); setError(caught instanceof BillingCsvError ? caught.message : '照合できませんでした。'); }
  }

  const headers = rows?.[0] ?? [];
  const problems = rows ? billingMappingProblems(mapping, headers.length) : [];
  const missing = new Set(BILLING_FIELDS.filter(spec => spec.required && mapping[spec.field] === undefined).map(spec => spec.field));
  const appliedTotal = applied.reduce((sum, period) => sum + (period.amountYen ?? 0), 0);
  const appliedUnknown = applied.filter(period => period.amountYen === null).length;

  return <section className="jc-billing" aria-labelledby={`${id}-title`}>
    <h3 id={`${id}-title`}>課金CSV</h3>
    <p className="jc-billing-volatile" role="note">読み込んだ課金はこの画面の中だけで使います。サーバーには送らず、ページを再読み込みすると消えます。</p>

    <ol className="jc-billing-steps">
      <li aria-current={!rows ? 'step' : undefined}><h4>1. ファイルを選ぶ</h4>
        <div className="jc-billing-row">
          <label>課金CSVファイル<input type="file" accept=".csv,text/csv" disabled={reading} onChange={event => { void choose(event); }} /></label>
          <label>文字コード<select value={encoding} onChange={event => { changeEncoding(event.target.value as EncodingChoice); }}>
            {(Object.keys(encodingLabels) as EncodingChoice[]).map(value => <option key={value} value={value}>{encodingLabels[value]}</option>)}
          </select></label>
        </div>
        <p className="jc-muted">必要な列: 媒体（Airワーク / HRハッカー）・媒体求人ID・期間開始・期間終了・金額。あれば使う列: プラン名・表示回数・クリック数・媒体の応募数。5MBまで。</p>
        {reading && <p role="status">読み込み中…</p>}
        {fileName && rows && <p role="status">{fileName}：{String(rows.length - 1)}行（{usedEncoding === 'shift_jis' ? 'Excel の日本語 CSV' : 'UTF-8'} として読み取り）</p>}
        {error && <p role="alert" className="jc-error">{error}</p>}
      </li>

      {rows && <li aria-current={!result ? 'step' : undefined}><h4>2. 列の対応を確かめる</h4>
        <p className="jc-muted">見出しから自動で選んでいます。違っていれば選び直してください。赤い項目は必須なのに列が選ばれていません。</p>
        <table className="jc-billing-mapping"><thead><tr><th scope="col">項目</th><th scope="col">CSV の列</th><th scope="col">1行目の値</th></tr></thead><tbody>
          {BILLING_FIELDS.map(spec => {
            const column = mapping[spec.field];
            const sample = column === undefined ? '' : rows[1]?.[column] ?? '';
            return <tr key={spec.field} className={missing.has(spec.field) ? 'jc-billing-missing' : undefined}>
              <th scope="row">{spec.label}{spec.required ? <span className="jc-billing-required">（必須）</span> : <span className="jc-muted">（任意）</span>}</th>
              <td><select aria-label={`${spec.label}の列`} value={column === undefined ? '' : String(column)} onChange={event => { changeColumn(spec.field, event.target.value); }}>
                <option value="">使わない</option>
                {headers.map((header, index) => <option key={`${String(index)}-${header}`} value={String(index)}>{header || `（${String(index + 1)}列目・見出しなし）`}</option>)}
              </select></td>
              <td>{sample}</td>
            </tr>;
          })}
        </tbody></table>
        <label className="jc-billing-tax">金額の扱い<select value={taxBasis} onChange={event => { setTaxBasis(event.target.value as BillingTaxBasis); setResult(null); }}>
          <option value="税込">税込</option><option value="税抜">税抜</option><option value="不明">わからない</option>
        </select></label>
        {problems.length > 0 && <ul className="jc-error" role="alert">{problems.map(problem => <li key={problem}>{problem}</li>)}</ul>}
        <button type="button" className="jc-button jc-primary" disabled={problems.length > 0} onClick={check}>求人と照合する</button>
      </li>}

      {result && <li aria-current={!appliedNow ? 'step' : undefined}><h4>3. 求人との照合結果</h4>
        <p className="jc-muted">媒体と媒体求人IDが一覧の求人と完全に同じ行だけを結びつけます。求人名からの推測はしません。</p>
        <dl className="jc-billing-counts" aria-label="照合結果の件数">
          <div><dt>一致</dt><dd>{String(result.counts.matched)}<small>行</small></dd></div>
          <div><dt>候補が複数</dt><dd>{String(result.counts.ambiguous)}<small>行</small></dd></div>
          <div><dt>一覧に無い</dt><dd>{String(result.counts.notFound)}<small>行</small></dd></div>
          <div><dt>値に誤り</dt><dd>{String(result.counts.rejected)}<small>行</small></dd></div>
          <div><dt>同じ期間の重複</dt><dd>{String(result.counts.duplicates)}<small>行</small></dd></div>
        </dl>
        <IssueList title="値に誤りがあり使わない行" issues={result.rejected} />
        <IssueList title="一覧に無い求人の行" issues={result.notFound} />
        <IssueList title="候補が複数あり結びつけない行" issues={result.ambiguous} />
        <IssueList title="同じ期間の重複（合算しません）" issues={result.duplicates} />
        <IssueList title="確認してほしい行" issues={result.warnings} />
      </li>}

      {result && <li aria-current={result.counts.matched > 0 && !appliedNow ? 'step' : undefined}><h4>4. 反映する</h4>
        <div className="jc-billing-row">
          <button type="button" className="jc-button jc-primary" disabled={result.counts.matched === 0} onClick={() => { onApply(result.periods); setAppliedNow(true); }}>一致した{String(result.counts.matched)}行を課金として反映</button>
          {applied.length > 0 && <button type="button" className="jc-button" onClick={() => { onClear(); setAppliedNow(false); }}>反映した課金を外す</button>}
        </div>
        {result.counts.matched === 0 && <p className="jc-muted">一致した行がないため反映できません。</p>}
      </li>}
    </ol>

    {applied.length > 0 && <p className="jc-billing-applied" role="status">
      課金CSVの {String(applied.length)} 期間を反映中（{String(new Set(applied.map(period => period.jobId)).size)}求人・{appliedUnknown === applied.length ? '金額はすべて不明' : `合計 ${yen(appliedTotal)}${appliedUnknown ? `（ほかに金額不明 ${String(appliedUnknown)}期間）` : ''}`}）。ページを再読み込みすると消えます。
    </p>}
  </section>;
}
