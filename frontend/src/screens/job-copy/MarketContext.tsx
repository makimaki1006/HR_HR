import { useEffect, useMemo, useState } from 'react';
import { apiGet } from '../../api/client';
import { EChart } from '../../components/EChart';
import type { JobCopyRecord } from './data';
import { marketRows, monthlyApplications, trendOption, type MarketData } from './marketChartModel';
import './job-analysis.css';

const amount = (value: number | null | undefined) => value == null ? '未取得' : value.toLocaleString('ja-JP', { maximumFractionDigits: 2 });
export function MarketContext({ job }: { job: JobCopyRecord }) {
  return <MarketContextForJob key={job.id} job={job} />;
}
function MarketContextForJob({ job }: { job: JobCopyRecord }) {
  const [data, setData] = useState<MarketData | null>(null);
  const [title, setTitle] = useState(''); const [prefecture, setPrefecture] = useState('');
  const [error, setError] = useState(''); const [loading, setLoading] = useState(true);
  const [range, setRange] = useState('all');
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    const controller = new AbortController();
    const params = title && prefecture ? `?${new URLSearchParams({ title, prefecture }).toString()}` : '';
    void apiGet<MarketData>(`/api/job-copy/market${params}`, { signal: controller.signal }).then(result => {
      if (controller.signal.aborted) return;
      if (result.ok) { setData(result.data); setError(''); }
      else { setError('市場データを取得できませんでした。接続・権限を確認してください。'); }
      setLoading(false);
    });
    return () => { controller.abort(); };
  }, [title, prefecture, retry]);
  const rows = useMemo(() => data?.series ? marketRows(data.series) : [], [data]);
  const shown = range === '12' ? rows.slice(-12) : rows;
  const applications = useMemo(() => monthlyApplications(job), [job]);
  const total = job.overallApplications;
  const charts = [
    { key: 'jobs' as const, label: '市場求人数', unit: '件' },
    { key: 'viewers' as const, label: '市場閲覧者指標（ctk）', unit: '指標値' },
    { key: 'employers' as const, label: '募集企業数', unit: '社' },
    { key: 'viewersPerJob' as const, label: '1求人当たり閲覧者指標', unit: '指標値/求人' },
  ];
  return <section className="jc-analysis jc-internal-market" aria-label="市場環境と応募獲得の要因">
    <h2>市場環境と応募獲得の要因</h2><p>{job.title} · {job.location}</p>
    <p className="jc-notice">内部分析用です。市場職種と都道府県を選んで比較します。市場の閲覧者指標は求職者数や、この求人の応募者数ではありません。</p>
    <section aria-label="この求人の応募推移"><h3>この求人の月別応募数</h3>
      {total && <p>取得済み応募：{amount(total.total)}件 · 応募日あり：{amount(total.total - total.missingDate)}件 · 応募日不明：{amount(total.missingDate)}件（グラフ対象外）</p>}
      {applications?.length ? <>
        <p>対象月：{applications[0]?.month}〜{applications.at(-1)?.month} · HubSpot求人レコードの応募日別集計。版との対応が不明な応募も含みます。</p>
        <EChart option={trendOption(applications.map(row => row.month), applications.map(row => row.count), 'この求人の応募数', '件', '#a36816', true, '日付付き応募の記録なし')} testId="jc-applications-monthly" renderer="svg" height={280} />
        <details><summary>月別応募数の数値を確認</summary><table><caption>応募日が分かる取得済み応募の月別集計</caption><thead><tr><th>対象月</th><th>応募数</th></tr></thead><tbody>{applications.map(row => <tr key={row.month}><th>{row.month}</th><td>{row.count === null ? '日付付き応募の記録なし' : `${amount(row.count)}件`}</td></tr>)}</tbody></table></details>
        <p>棒がない月はこの集計に日付付き応募がありません。未収集の応募や媒体上の全応募が0だったことを示すものではありません。取得：{total?.fetchedAt.slice(0, 10)}</p>
      </> : <p>{total?.total === 0 ? '取得済み応募は0件です。応募の対象月がないためグラフは表示しません。' : total && total.total === total.missingDate ? '取得済み応募の応募日がすべて不明のため、月別応募グラフは表示できません。' : '応募日別集計が未取得のため、月別応募グラフは表示できません。'} 応募数を観測日に割り当てません。</p>}
    </section>
    <h3>職種・地域の市場推移</h3>
    {loading && <p role="status">市場データを取得中…</p>}{error && <><p role="alert">{error}</p><button onClick={() => { setLoading(true); setError(''); setRetry(value => value + 1); }} disabled={loading}>市場データを再取得</button></>}
    {data && <div className="jc-analysis-controls"><label>比較する市場職種<select value={title} onChange={event => { setLoading(true); setTitle(event.target.value); }}><option value="">選択してください</option>{data.titles.map(value => <option key={value}>{value}</option>)}</select></label><label>比較する都道府県<select value={prefecture} onChange={event => { setLoading(true); setPrefecture(event.target.value); }}><option value="">選択してください</option>{data.prefectures.map(value => <option key={value}>{value}</option>)}</select></label></div>}
    {!loading && !error && data && <><p>{data.source}</p><p>{data.ctk_basis}</p>
      {shown.length > 0 ? <>
        <div className="jc-analysis-controls"><label>市場グラフの表示期間<select value={range} onChange={event => { setRange(event.target.value); }}><option value="all">取得済みの全期間</option><option value="12">取得済みの最新12か月</option></select></label></div>
        <p>市場の対象：{title} / {data.series?.prefecture} · {shown[0]?.month}〜{shown.at(-1)?.month}。グラフごとに単位と縦軸が異なります。未取得月は線をつなぎません。</p>
        <p>応募と市場の取得期間は一致するとは限りません。それぞれの対象月を確認して比較してください。</p>
        <div className="jc-market-charts">{charts.map(chart => <section key={chart.key} aria-label={`${chart.label}の月次グラフ`}><h4>{chart.label}</h4>
          {shown.some(row => row[chart.key] !== null) ? <EChart option={trendOption(shown.map(row => row.month), shown.map(row => row[chart.key]), chart.label, chart.unit)} testId={`jc-market-${chart.key}`} renderer="svg" height={280} /> : <p>この指標は未取得です。</p>}
        </section>)}</div>
        <details><summary>市場実績の数値表を確認</summary><p className="jc-analysis-scroll-hint">表は横にスクロールして確認できます。</p><div className="jc-analysis-table" role="region" aria-label="市場実績の数値表" tabIndex={0}><table><caption>{title} / {data.series?.prefecture}の月次市場実績</caption><thead><tr><th>対象月</th><th>市場求人数</th><th>市場閲覧者指標（ctk）</th><th>募集企業数</th><th>1求人当たり閲覧者指標</th></tr></thead><tbody>{shown.map(row => <tr key={row.month}><th>{row.month}</th><td>{amount(row.jobs)}</td><td>{amount(row.viewers)}</td><td>{amount(row.employers)}</td><td>{amount(row.viewersPerJob)}</td></tr>)}</tbody></table></div></details>
      </> : <p>{title && prefecture ? '選択した職種・県の月次市場データはありません。' : '市場職種と県を選択するとグラフを表示します。'} 未取得値を0には置き換えません。</p>}
    </>}
    <details className="jc-market-observations"><summary>本文・画像の観測日を確認（{job.versions.filter(version => version.kind === 'published').length}版）</summary><p>観測日は確認できた日です。実際の変更日とは限りません。応募月と見比べても、この時点で変更効果は確定できません。</p><ul>{job.versions.filter(version => version.kind === 'published').map(version => <li key={version.id}>{version.observedAt.slice(0, 10)} · {version.label} · 画像参照 {version.images?.length ?? version.imageReferences?.length ?? 0}件</li>)}</ul></details>
    <h3>同時に確認する変数</h3><p>文面・画像、給与や勤務条件、課金額・表示量、職種と地域の市場環境、掲載期間、選考の連絡速度、会社の認知・評判を並べて確認します。複数の変数が同時に変わるため、応募数の変化だけで原因を確定しません。</p><details><summary>会社のブランド力の根拠を整理する（画面内メモ）</summary><label>確認した根拠<textarea rows={4} placeholder="企業名検索、採用ページの閲覧、応募理由の言及、認知調査など。取得日と出典も記録" /></label><p>ブランド力の実測値は未接続です。このメモは保存されません。</p></details>
  </section>;
}
