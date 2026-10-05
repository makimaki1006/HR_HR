import { useEffect, useState } from 'react';
import { apiGet } from '../../api/client';
import type { JobCopyRecord } from './data';
import './job-analysis.css';

interface MarketData { source: string; titles: string[]; prefectures: string[]; ctk_basis: string; series: { prefecture: string; months: string[]; job_count: (number | null)[]; ctk_count: (number | null)[]; employer_count: (number | null)[]; seekers_per_posting: (number | null)[] } | null }
const amount = (value: number | null | undefined) => value === null || value === undefined ? '未取得' : value.toLocaleString('ja-JP', { maximumFractionDigits: 2 });
export function MarketContext({ job }: { job: JobCopyRecord }) {
  const [data, setData] = useState<MarketData | null>(null);
  const [title, setTitle] = useState(''); const [prefecture, setPrefecture] = useState('');
  const [error, setError] = useState(''); const [loading, setLoading] = useState(true);
  useEffect(() => {
    const controller = new AbortController();
    const params = title && prefecture ? `?${new URLSearchParams({ title, prefecture }).toString()}` : '';
    void apiGet<MarketData>(`/api/job-copy/market${params}`, { signal: controller.signal }).then(result => {
      if (controller.signal.aborted) return;
      if (result.ok) { setData(result.data); setError(''); }
      else { setData(null); setError('市場データを取得できませんでした。接続・権限を確認してください。'); }
      setLoading(false);
    });
    return () => { controller.abort(); };
  }, [title, prefecture]);
  return <section className="jc-analysis jc-internal-market" aria-label="市場環境と応募獲得の要因"><h2>市場環境と応募獲得の要因</h2><p>{job.title} · {job.location}</p><p className="jc-notice">内部分析用です。求人と市場レポートの職種・県を明示的に選びます。市場人数は、この求人の応募者数ではありません。</p>
    {loading && <p role="status">市場データを取得中…</p>}{error && <p role="alert">{error}</p>}
    {data?.series && <p className="jc-analysis-scroll-hint">市場実績の表は横にスクロールして確認できます。</p>}
    {data && <><div className="jc-analysis-controls"><label>比較する市場職種<select value={title} onChange={event => { setLoading(true); setTitle(event.target.value); }}><option value="">選択してください</option>{data.titles.map(value => <option key={value}>{value}</option>)}</select></label><label>比較する都道府県<select value={prefecture} onChange={event => { setLoading(true); setPrefecture(event.target.value); }}><option value="">選択してください</option>{data.prefectures.map(value => <option key={value}>{value}</option>)}</select></label></div><p>{data.source}</p><p>{data.ctk_basis}</p>{data.series ? <div className="jc-analysis-table"><table><caption>{title} / {data.series.prefecture}の月次市場実績</caption><thead><tr><th>対象月</th><th>市場求人数</th><th>市場閲覧者指標（ctk）</th><th>募集企業数</th><th>1求人当たり閲覧者指標</th></tr></thead><tbody>{data.series.months.map((month, index) => <tr key={month}><th>{month}</th><td>{amount(data.series?.job_count[index])}</td><td>{amount(data.series?.ctk_count[index])}</td><td>{amount(data.series?.employer_count[index])}</td><td>{amount(data.series?.seekers_per_posting[index])}</td></tr>)}</tbody></table></div> : <p>市場職種と県を選択してください。該当する月次データがない場合も0に置き換えません。</p>}</>}
    <h3>同時に確認する変数</h3><p>文面・画像、給与や勤務条件、課金額・表示量、職種と地域の市場環境、掲載期間、選考の連絡速度、会社の認知・評判を並べて確認します。複数の変数が同時に変わるため、応募数の変化だけで原因を確定しません。</p><details><summary>会社のブランド力の根拠を整理する（画面内メモ）</summary><label>確認した根拠<textarea rows={4} placeholder="企業名検索、採用ページの閲覧、応募理由の言及、認知調査など。取得日と出典も記録" /></label><p>ブランド力の実測値は未接続です。このメモは保存されません。</p></details>
  </section>;
}
