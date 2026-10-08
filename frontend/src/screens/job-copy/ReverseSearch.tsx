import { useState } from 'react';
import type { JobCopyRecord } from './data';
import { reverseSearch, reverseSearchOptions } from './reverseSearchModel';
import type { ReverseSearchQuery } from './reverseSearchModel';
import { formatDateTimeJst, orderAgeBands, orderPrefectures } from './format';
import './job-analysis.css';

const labels = { gender: '応募者の性別', age: '応募者の年代', prefecture: '応募者の都道府県', municipality: '応募者の市区町村' };
const optionOrder = (key: keyof typeof labels, values: string[]) => key === 'age' ? orderAgeBands(values) : key === 'prefecture' || key === 'municipality' ? orderPrefectures(values) : values;
/**
 * 応募者の条件から求人を探す。一覧の見出しのボタンから開くパネル（主作業の流れの外に置く）。
 * 地域は都道府県と市区町村までに丸めた値だけを選択肢に出す。
 */
export function ReverseSearch({ records, onChoose, onClose }: { records: JobCopyRecord[]; onChoose: (job: JobCopyRecord) => void; onClose: () => void }) {
  const [query, setQuery] = useState<ReverseSearchQuery>({ gender: '', age: '', prefecture: '', municipality: '', minimum: 1 });
  const results = reverseSearch(records, query);
  const covered = records.filter(job => job.jointDemographics);
  return <section className="jc-analysis jc-reverse-search" id="job-copy-reverse-search" aria-labelledby="job-copy-reverse-search-heading"><div className="jc-panel-heading"><h2 id="job-copy-reverse-search-heading" tabIndex={-1}>応募者の条件から求人を探す</h2><button type="button" className="jc-button" onClick={onClose}>閉じる</button></div><p>応募者の地域・年代・性別を同時に絞り、該当人数が多い順に表示します。勤務地の条件とは別です。性別・年代・地域の組み合わせ集計がある求人 {covered.length} / {records.length}件。</p><p>応募者の地域は都道府県と市区町村までに丸めています。3人未満の地域と、性別・年代と組み合わせると3人未満になる地域は「その他」にまとめています。そのため、地域で絞った人数は応募者の構成の表より少なくなることがあります。</p><p className="jc-notice">対象は取得済み応募全期間です。現在の求人内容への反応と断定できません。将来の自然言語検索はこの集計を参照する設計で、AIはまだつないでいません。</p>
    <div className="jc-analysis-controls">{(Object.keys(labels) as (keyof typeof labels)[]).map(key => <label key={key}>{labels[key]}<select aria-label={labels[key]} value={query[key]} onChange={event => { setQuery(current => ({ ...current, [key]: event.target.value })); }}><option value="">すべて</option>{optionOrder(key, reverseSearchOptions(covered, key)).map(value => <option key={value}>{value}</option>)}</select></label>)}<label>最低該当人数<input type="number" min="1" step="1" value={query.minimum} onChange={event => { setQuery(current => ({ ...current, minimum: Number(event.target.value) })); }} /></label></div>
    {!covered.length ? <p>性別・年代・地域を組み合わせた集計が未取得です。年代別と性別の集計を掛け合わせて人数を作りません。</p> : !results.length ? <p role="status">条件に一致する求人はありません。未取得求人は検索結果に含めていません。</p> : <div aria-label="逆検索の結果">{results.map(result => <button className="jc-analysis-result" key={result.job.id} onClick={() => { onChoose(result.job); }}><strong>{result.job.title}</strong> · {result.job.company}<br />該当{result.count}件 / この求人の全応募{result.denominator}件（{result.percentage?.toFixed(1) ?? '—'}%）<br /><small>応募集計取得：{formatDateTimeJst(result.job.overallApplications?.fetchedAt, '未取得')} · 求人を開く</small></button>)}</div>}
  </section>;
}
