import type { PopulationBand } from '../../../generated/PopulationBand';
import type { PopulationSection } from '../../../generated/PopulationSection';
import type { PopulationShares } from '../../../generated/PopulationShares';
import { Pyramid } from '../charts/Pyramid';
import { SharesPyramid } from '../charts/SharesPyramid';
import { fixed, fmtInt, fmtNumber, MISSING } from '../format';

const GROUP_COLORS = ['#5a9cb0', '#007d79', '#8567a5', '#a8b2bb'];

function wageSource(source: string): string {
  if (source === 'official_csv') return '厚生労働省の公式改定一覧';
  if (source === 'database') return '外部統計データベース';
  return MISSING;
}

export function PopulationTab({ data }: { data: PopulationSection }) {
  if (data.status === 'unavailable') {
    return (
      <section className="cmp-page">
        <h2>人口・地域データ</h2>
        <p>集計地域：{data.region}</p>
        <p className="cmp-note">{data.message}</p>
      </section>
    );
  }
  const { region, bands, labor, shares } = data;
  const year = (y: number | null | undefined): string => (y === null || y === undefined ? MISSING : String(y));
  return (
    <section className="cmp-page">
      <h2>人口・地域データ</h2>
      <p>集計地域：{region}</p>
      <p className="cmp-note">出典：国勢調査・厚生労働省・e-Stat等の保存統計。</p>
      <p className="cmp-note">人口の基準日：{data.reference_date ?? '未取得'}</p>
      {shares === null ? <BandsOnly bands={bands} /> : <Shares shares={shares} />}
      {!data.is_national && (
        <>
          <h3>地域の最低賃金・労働統計</h3>
          <table className="cmp-table">
            <tbody>
              <tr>
                <th scope="row">最低賃金（円/時）</th>
                <td>{fmtNumber(data.minimum_wage)}</td>
              </tr>
              <tr>
                <th scope="row">最低賃金の改定年度</th>
                <td>{year(data.minimum_wage_fiscal_year)}</td>
              </tr>
              <tr>
                <th scope="row">最低賃金の発効日</th>
                <td>{data.minimum_wage_effective_date}</td>
              </tr>
              <tr>
                <th scope="row">最低賃金の基準日（日本時間）</th>
                <td>{data.minimum_wage_as_of}</td>
              </tr>
              <tr>
                <th scope="row">最低賃金の出典</th>
                <td>
                  {wageSource(data.minimum_wage_source)}
                  {data.minimum_wage_source_url !== null && (
                    <>
                      {' / '}
                      <a href={data.minimum_wage_source_url} target="_blank" rel="noopener noreferrer">
                        公式資料を確認
                      </a>
                    </>
                  )}
                </td>
              </tr>
              <tr>
                <th scope="row">労働統計の年度</th>
                <td>{year(labor?.fiscal_year)}</td>
              </tr>
              <tr>
                <th scope="row">完全失業率（%）</th>
                <td>{fmtNumber(labor?.unemployment_rate ?? null)}</td>
              </tr>
              <tr>
                <th scope="row">離職率（%）</th>
                <td>{fmtNumber(labor?.separation_rate ?? null)}</td>
              </tr>
            </tbody>
          </table>
          <p className="cmp-note">取得できない指標は — と表示します。</p>
        </>
      )}
    </section>
  );
}

/** 構成比が成立しないとき (総人口が無い・男女合計と合わない等)。割合は出さず、人数だけ。欠測は — のまま。 */
function BandsOnly({ bands }: { bands: readonly PopulationBand[] }) {
  const complete = bands.flatMap((b) =>
    b.male === null || b.female === null ? [] : [{ age_group: b.age_group, male: b.male, female: b.female }],
  );
  const hasGap = complete.length !== bands.length;
  const cell = (n: number | null): string => (n === null ? MISSING : fmtInt(n));
  return (
    <div className="cmp-population-grid">
      <div>
        <h3>人口ピラミッド</h3>
        {bands.length === 0 ? (
          <p className="cmp-note">人口データがありません。</p>
        ) : hasGap ? (
          <p className="cmp-note">欠測のためグラフの表示を保留。取得済みの人数は表に表示します。</p>
        ) : (
          <>
            <p className="cmp-note">左：男性 / 右：女性</p>
            <Pyramid bands={complete} />
          </>
        )}
      </div>
      <div>
        <h3>年齢別人口</h3>
        <table className="cmp-table">
          <thead>
            <tr>
              <th>年齢</th>
              <th>男性</th>
              <th>女性</th>
              <th>合計</th>
            </tr>
          </thead>
          <tbody>
            {bands.map((b, i) => (
              <tr key={i}>
                <td>{b.age_group}</td>
                <td className="cmp-num">{cell(b.male)}</td>
                <td className="cmp-num">{cell(b.female)}</td>
                <td className="cmp-num">{b.male === null || b.female === null ? MISSING : fmtInt(b.male + b.female)}</td>
              </tr>
            ))}
            {bands.length === 0 && (
              <tr>
                <td colSpan={4}>データなし</td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/** 総人口を分母にした構成比。年齢別の合計との差は「年齢区分未収録」として別に出す。 */
function Shares({ shares }: { shares: PopulationShares }) {
  const metrics: [string, number, number][] = [
    ['総人口', shares.total, 100],
    ['男性', shares.male, shares.male_share_pct],
    ['女性', shares.female, shares.female_share_pct],
  ];
  return (
    <>
      <div className="cmp-population-metrics">
        {metrics.map(([label, n, pct]) => (
          <article key={label}>
            {label}
            <strong>{fmtInt(n)}人</strong>
            <span>総人口の{fixed(pct, 1)}%</span>
          </article>
        ))}
      </div>
      {shares.age_groups !== null && (
        <>
          <h3>年齢3区分の構成比</h3>
          <div className="cmp-age-composition" role="img" aria-label="年齢3区分と年齢区分未収録分の構成比">
            {shares.age_groups.map((g, i) => (
              <span
                key={g.label}
                style={{ width: `${g.share_pct.toFixed(8)}%`, background: GROUP_COLORS[i] }}
                title={`${g.label}：${fmtInt(g.count)}人 / ${fixed(g.share_pct, 2)}%`}
              />
            ))}
          </div>
          <div className="cmp-population-legend">
            {shares.age_groups.map((g, i) => (
              <span key={g.label}>
                <i style={{ background: GROUP_COLORS[i] }} />
                {g.label} {fixed(g.share_pct, 1)}%
              </span>
            ))}
          </div>
        </>
      )}
      <h3>年齢別・男女別の構成比</h3>
      <figure className="cmp-trend-card">
        <div className="cmp-trend-plot">
          <SharesPyramid shares={shares} />
        </div>
      </figure>
      <p className="cmp-note">構成比の分母：地域の総人口。</p>
      <table className="cmp-table">
        <thead>
          <tr>
            <th>年齢</th>
            <th>男性（人）</th>
            <th>女性（人）</th>
            <th>合計（人）</th>
            <th>構成比</th>
          </tr>
        </thead>
        <tbody>
          {[...shares.bands, shares.unrecorded].map((b) => (
            <tr key={b.age_group}>
              <td>{b.age_group}</td>
              <td className="cmp-num">{fmtInt(b.male)}</td>
              <td className="cmp-num">{fmtInt(b.female)}</td>
              <td className="cmp-num">{fmtInt(b.male + b.female)}</td>
              <td className="cmp-num">{fixed(b.total_share_pct, 2)}%</td>
            </tr>
          ))}
          <tr>
            <th scope="row">合計</th>
            <td className="cmp-num">{fmtInt(shares.male)}</td>
            <td className="cmp-num">{fmtInt(shares.female)}</td>
            <td className="cmp-num">{fmtInt(shares.total)}</td>
            <td className="cmp-num">100.00%</td>
          </tr>
        </tbody>
      </table>
    </>
  );
}
