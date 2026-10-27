import type { GoogleKeyword } from '../../../generated/GoogleKeyword';
import type { GoogleSection } from '../../../generated/GoogleSection';
import type { GoogleSeason } from '../../../generated/GoogleSeason';
import { MultiLineChart } from '../charts/MultiLineChart';
import { SeasonChart } from '../charts/SeasonChart';
import type { SeasonLineInput } from '../charts/seasonLayout';
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
  const { keyword, region, demand, suggestions, season } = data;
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
          {season !== null && <SeasonBlock s={season} />}
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

/** 「暦月の型（全国）」。言えるときは職種の型を主に、言えないときは市場全体の型を主にする。 */
function SeasonBlock({ s }: { s: GoogleSeason }) {
  const googleLines: SeasonLineInput[] = s.months.map((m) => ({
    month: m.month,
    mean: m.mean,
    range: [m.min, m.max],
  }));
  const market = s.market;
  const marketLines: SeasonLineInput[] =
    market === null ? [] : market.months.map((m) => ({ month: m.month, mean: m.mean, range: null }));
  return (
    <div id="competitor-google-season">
      <div className="cmp-block-title">暦月の型（全国）</div>
      <p className="cmp-note">
        {s.keyword} の全国の検索数。完全な年（12 か月そろった年）{s.full_years.length} 年分から、年平均を 100 とした暦月の指数を出しています。
      </p>
      <p>
        <strong>{s.headline}</strong>
      </p>
      {s.caveat !== null && <p className="cmp-note">{s.caveat}</p>}
      {!s.reliable && market !== null && (
        <>
          <p>
            {market.headline}（{market.n_series} 職種の平均）
          </p>
          <SeasonChart
            label="職種全体の暦月の型"
            lineName="職種全体の平均"
            color="#5b6770"
            lines={marketLines}
            currentMonth={s.current_month}
            indeed={null}
          />
        </>
      )}
      <SeasonChart
        label={s.reliable ? `${s.keyword}：暦月の型` : `${s.keyword}：暦月の指数（参考）`}
        lineName="Google 検索数の平均"
        color="#007d79"
        lines={googleLines}
        currentMonth={s.current_month}
        indeed={s.indeed}
      />
      {s.indeed !== null && (
        <>
          <p className="cmp-note">{s.indeed.span_note}</p>
          <p className="cmp-note">{s.indeed.shift_note}</p>
        </>
      )}
      {s.reliable && s.next_three.length > 0 && (
        <table className="cmp-table">
          <thead>
            <tr>
              <th>月</th>
              <th>年平均比</th>
            </tr>
          </thead>
          <tbody>
            {s.current_index !== null && (
              <tr>
                <td>{s.current_month}月（今月）</td>
                <td className="cmp-num">{Math.round(s.current_index)}</td>
              </tr>
            )}
            {s.next_three.map((n) => (
              <tr key={n.month}>
                <td>{n.month}月</td>
                <td className="cmp-num">{Math.round(n.index)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {s.reliable && market !== null && (
        <p className="cmp-note">
          {market.headline}（{market.n_series} 職種の平均）
        </p>
      )}
    </div>
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
