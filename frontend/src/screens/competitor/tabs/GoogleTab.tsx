import { useState } from 'react';
import type { GoogleKeyword } from '../../../generated/GoogleKeyword';
import type { GoogleSection } from '../../../generated/GoogleSection';
import { TrendChart } from '../charts/TrendChart';
import { fmtNumber } from '../format';

// 生のエラー本文は資格情報を含みうるので、Rust 側が落としている。ここは固定文だけを出す。
const DEMAND_FAILED = 'Google検索需要を取得できませんでした。';

export function GoogleTab({ data }: { data: GoogleSection }) {
  return (
    <section className="cmp-page" id="competitor-google">
      <header className="cmp-page-head">
        <span className="cmp-eyebrow">検索需要</span>
        <h2>Google広告APIの検索需要</h2>
        <p className="cmp-sub">月間検索数と推移</p>
      </header>
      <p className="cmp-note">出典：Google広告 Keyword Planner API｜検索数は推定値。広告競合度は広告主間の競合です。</p>
      {data.status === 'ok' ? <GoogleBody data={data} /> : <p>{data.message}</p>}
    </section>
  );
}

function GoogleBody({ data }: { data: Extract<GoogleSection, { status: 'ok' }> }) {
  const { keyword, region, demand, suggestions } = data;
  return (
    <>
      <p>
        検索語: {keyword} / 指定地域: {region === '' ? '全国' : region}
      </p>
      {demand.status === 'ok' ? (
        <>
          {demand.region_name === null ? (
            region !== '' && (
              <p className="cmp-note">指定地域を解決できなかったため全国の検索需要です。</p>
            )
          ) : (
            <p className="cmp-note">取得地域: {demand.region_name}</p>
          )}
          <table className="cmp-table">
            <thead>
              <tr>
                <th>検索語</th>
                <th>平均月間検索数</th>
                <th>広告競合度</th>
              </tr>
            </thead>
            <tbody>
              {demand.keywords.length === 0 && (
                <tr>
                  <td colSpan={3}>検索需要のデータがありません。</td>
                </tr>
              )}
              {demand.keywords.map((row, i) => (
                <tr key={i}>
                  <td>{row.keyword}</td>
                  <td className="cmp-num">{fmtNumber(row.avg_monthly)}</td>
                  <td>{row.competition}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {demand.keywords.map((row, i) => (
            <div key={i}>
              <TrendChart
                label={`${row.keyword}：月間検索数の推移`}
                unit="回/月"
                color="#007d79"
                points={row.monthly_12m.map((m) => ({ month: m.month, value: m.search_volume }))}
              />
              <MonthlyTable row={row} />
            </div>
          ))}
        </>
      ) : (
        <p className="cmp-note">{DEMAND_FAILED}</p>
      )}
      {suggestions.status === 'ok' && (
        <>
          <div className="cmp-block-title">関連キーワード（上位20件）</div>
          <p className="cmp-note">
            {suggestions.region_name === null
              ? '関連キーワードの取得地域：全国（地域指定なし・地域未解決）。指定地域の需要とは限りません。'
              : `関連キーワードの取得地域：${suggestions.region_name}`}
          </p>
          <table className="cmp-table">
            <thead>
              <tr>
                <th>関連語</th>
                <th>平均月間検索数</th>
              </tr>
            </thead>
            <tbody>
              {suggestions.suggestions.length === 0 && (
                <tr>
                  <td colSpan={2}>関連キーワードのデータがありません。需要0を意味しません。</td>
                </tr>
              )}
              {suggestions.suggestions.map((row, i) => (
                <tr key={i}>
                  <td>{row.keyword}</td>
                  <td className="cmp-num">{fmtNumber(row.avg_monthly)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="cmp-note">求人票の訴求語と検索語を比較できます。</p>
        </>
      )}
    </>
  );
}

/** 語ごとの 12 か月の表。語が多いと縦に長くなるので、開くまで行を DOM に出さない。 */
function MonthlyTable({ row }: { row: GoogleKeyword }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="cmp-monthly">
      <button
        type="button"
        className="cmp-disclosure"
        aria-expanded={open}
        onClick={() => {
          setOpen((v) => !v);
        }}
      >
        {row.keyword} の月別検索数
      </button>
      {open && (
        <table className="cmp-table">
          <thead>
            <tr>
              <th>月</th>
              <th>検索数</th>
            </tr>
          </thead>
          <tbody>
            {row.monthly_12m.map((m, i) => (
              <tr key={i}>
                <td>{m.month}</td>
                <td className="cmp-num">{fmtNumber(m.search_volume)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
