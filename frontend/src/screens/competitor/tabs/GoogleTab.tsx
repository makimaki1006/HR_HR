import type { GoogleKeyword } from '../../../generated/GoogleKeyword';
import type { GoogleSection } from '../../../generated/GoogleSection';
import { MultiLineChart } from '../charts/MultiLineChart';
import { fixed, fmtInt, fmtNumber, MISSING } from '../format';
import { allMonths, keywordLines, monthYearPivot, summarize, yearOverlay } from '../googleStats';

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
          <DemandView keywords={demand.keywords} />
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

const signed = (v: number): string => `${v >= 0 ? '+' : '-'}${fixed(Math.abs(v), 1)}`;

/** 先頭の検索語がメイン、続くのが近隣の検索語。 */
function DemandView({ keywords }: { keywords: GoogleKeyword[] }) {
  const main = keywords[0];
  const months = allMonths(keywords);
  const rows = keywords.map(summarize);
  const overlay = main ? yearOverlay(main) : null;
  const pivot = main ? monthYearPivot(main) : null;
  return (
    <>
      <div className="cmp-block-title">検索語ごとの要約</div>
      <table className="cmp-table">
        <thead>
          <tr>
            <th>検索語</th>
            <th>平均月間検索数</th>
            <th>直近12か月の合計</th>
            <th>前年同期比</th>
            <th>ピーク月</th>
            <th>広告競合度</th>
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 && (
            <tr>
              <td colSpan={6}>検索需要のデータがありません。</td>
            </tr>
          )}
          {rows.map((r, i) => (
            <tr key={i}>
              <td>{r.keyword}</td>
              <td className="cmp-num">{fmtNumber(r.avgMonthly)}</td>
              <td className="cmp-num">{r.last12Total === null ? MISSING : fmtInt(r.last12Total)}</td>
              <td className="cmp-num">{r.yoyPct === null ? MISSING : `${signed(r.yoyPct)}%`}</td>
              <td>{r.peakMonth === null ? MISSING : `${String(r.peakMonth)}月`}</td>
              <td>{r.competition}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="cmp-note">
        前年同期比は、直近12か月とその前の12か月がそろっている検索語だけ出します。ピーク月は、同じ月どうしの平均が最も大きい月です。
      </p>
      {months.length > 0 && (
        <MultiLineChart
          label="月間検索数の推移（近隣の検索語を含む）"
          months={months}
          lines={keywordLines(keywords, months)}
          unit="回/月"
        />
      )}
      {main && overlay && overlay.years.length > 0 && (
        <>
          <MultiLineChart
            label={`${main.keyword}：年ごとの重ね合わせ`}
            months={overlay.months}
            lines={overlay.lines}
            unit="回/月"
          />
          {pivot && (
            <>
              <div className="cmp-block-title">{main.keyword} の月別検索数</div>
              <table className="cmp-table">
                <thead>
                  <tr>
                    <th>月</th>
                    {pivot.years.map((y) => (
                      <th key={y}>{y}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {pivot.rows.map((row, m) => (
                    <tr key={m}>
                      <td>{m + 1}月</td>
                      {row.map((v, j) => (
                        <td key={j} className="cmp-num">
                          {fmtNumber(v)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          )}
        </>
      )}
    </>
  );
}
