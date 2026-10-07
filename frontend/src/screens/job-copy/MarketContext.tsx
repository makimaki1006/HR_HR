import { useEffect, useMemo, useState } from 'react';
import { apiGet } from '../../api/client';
import { EChart } from '../../components/EChart';
import type { JobCopyRecord } from './data';
import { marketRows, trendOption, type MarketData } from './marketChartModel';
import { AssumptionsNote } from './AssumptionsNote';
import { formatDateJst, joinPresent, plainWording } from './format';
import './job-analysis.css';

const amount = (value: number | null | undefined) => value == null ? '未取得' : value.toLocaleString('ja-JP', { maximumFractionDigits: 2 });
export function MarketContext({ job, view = 'charts' }: { job: JobCopyRecord; view?: 'charts' | 'table' | 'inactive' }) {
  return <MarketContextForJob key={job.id} job={job} view={view} />;
}
function MarketContextForJob({ job, view }: { job: JobCopyRecord; view: 'charts' | 'table' | 'inactive' }) {
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
  const charts = [
    { key: 'jobs' as const, label: '市場求人数', unit: '件' },
    { key: 'viewers' as const, label: 'Indeed閲覧者指標', unit: '指標値' },
    { key: 'employers' as const, label: '募集企業数', unit: '社' },
    { key: 'viewersPerJob' as const, label: '1求人当たり閲覧者指標', unit: '指標値/求人' },
  ];
  return <section className="jc-analysis jc-internal-market" aria-label="市場環境と応募獲得の要因">
    <h2>{view === 'table' ? '市場データの数値表' : '職種・地域の市場グラフ'}</h2><p>{job.title} · {job.location}</p>
    <AssumptionsNote className="jc-notice" includeHubSpot={false} summary="市場の数字はIndeedの集計です。閲覧者指標は求職者の人数や、この求人の応募数ではありません。" items={[
      data?.ctk_basis && plainWording(data.ctk_basis),
      'グラフごとに単位と縦軸が異なります。未取得の月は線をつなぎません。',
      '応募と市場の取得期間は一致するとは限りません。それぞれの対象月を確認して比べてください。',
      '応募数はHubSpotに記録されたものだけで、市場の数字とは出所が異なります。',
    ]} />
    {loading && <p role="status">市場データを取得中…</p>}{error && <><p role="alert">{error}</p><button onClick={() => { setLoading(true); setError(''); setRetry(value => value + 1); }} disabled={loading}>市場データを再取得</button></>}
    {data && <div className="jc-analysis-controls"><label>比較する市場職種<select value={title} onChange={event => { setLoading(true); setTitle(event.target.value); }}><option value="">選択してください</option>{data.titles.map(value => <option key={value}>{value}</option>)}</select></label><label>比較する都道府県<select value={prefecture} onChange={event => { setLoading(true); setPrefecture(event.target.value); }}><option value="">選択してください</option>{data.prefectures.map(value => <option key={value}>{value}</option>)}</select></label></div>}
    {!loading && !error && data && <><p>出典：{plainWording(data.source)}</p>
      {shown.length > 0 ? <>
        <div className="jc-analysis-controls"><label>市場グラフの表示期間<select value={range} onChange={event => { setRange(event.target.value); }}><option value="all">取得済みの全期間</option><option value="12">取得済みの最新12か月</option></select></label></div>
        <p>市場の対象：{joinPresent([title, data.series?.prefecture], ' / ')} · {formatDateJst(shown[0]?.month)}〜{formatDateJst(shown.at(-1)?.month)}</p>
        {view === 'charts' && <div className="jc-market-charts">{charts.map(chart => <section key={chart.key} aria-label={`${chart.label}の月次グラフ`}><h4>{chart.label}</h4>
          {shown.some(row => row[chart.key] !== null) ? <EChart option={{ ...trendOption(shown.map(row => row.month), shown.map(row => row[chart.key]), chart.label, chart.unit), dataZoom: [], grid: { top: 36, left: 14, right: 18, bottom: 30, containLabel: true } }} testId={`jc-market-${chart.key}`} renderer="svg" height={280} /> : <p>この指標は未取得です。</p>}
        </section>)}</div>}
        {view === 'table' && <><p className="jc-analysis-scroll-hint">表は横にスクロールして確認できます。</p><div className="jc-analysis-table" role="region" aria-label="市場実績の数値表" tabIndex={0}><table><caption>{title} / {data.series?.prefecture}の月次市場実績</caption><thead><tr><th>対象月</th><th>市場求人数</th><th title="Indeed上で求人を見た人の動きをもとにした指標です。求職者の人数や応募数ではありません。">Indeed閲覧者指標</th><th>募集企業数</th><th>1求人当たり閲覧者指標</th></tr></thead><tbody>{shown.map(row => <tr key={row.month}><th>{row.month}</th><td>{amount(row.jobs)}</td><td>{amount(row.viewers)}</td><td>{amount(row.employers)}</td><td>{amount(row.viewersPerJob)}</td></tr>)}</tbody></table></div></>}
      </> : <p>{title && prefecture ? '選択した職種・県の月次市場データはありません。' : '市場職種と県を選択するとグラフを表示します。'} 未取得値を0には置き換えません。</p>}
    </>}
  </section>;
}
