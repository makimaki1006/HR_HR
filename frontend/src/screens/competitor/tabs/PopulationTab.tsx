import type { PopulationSection } from '../../../generated/PopulationSection';
import { Pyramid } from '../charts/Pyramid';
import { fmtInt, fmtNumber, MISSING } from '../format';

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
        <p>集計地域：</p>
        <p className="cmp-note">{data.message}</p>
      </section>
    );
  }
  const { region, bands, labor } = data;
  const year = (y: number | null | undefined): string => (y === null || y === undefined ? MISSING : String(y));
  return (
    <section className="cmp-page">
      <h2>人口・地域データ</h2>
      <p>集計地域：{region}</p>
      <p className="cmp-note">
        出典：国勢調査（人口）、厚生労働省（最低賃金）、e-Stat社会人口統計体系・労働政策研究・研修機構（労働統計）。都道府県単位の外部統計です。地域の人口は求人閲覧人数・検索数・応募数とは異なります。統計ごとに調査時点は異なります。
      </p>
      <div className="cmp-population-grid">
        <div>
          <h3>人口ピラミッド</h3>
          {bands.length === 0 ? (
            <p className="cmp-note">人口データがありません。</p>
          ) : (
            <>
              <p className="cmp-note">左：男性 / 右：女性</p>
              <Pyramid bands={bands} />
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
              {bands.map((b) => (
                <tr key={b.age_group}>
                  <td>{b.age_group}</td>
                  <td className="cmp-num">{fmtInt(b.male)}</td>
                  <td className="cmp-num">{fmtInt(b.female)}</td>
                  <td className="cmp-num">{fmtInt(b.male + b.female)}</td>
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
            <td>{wageSource(data.minimum_wage_source)}</td>
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
    </section>
  );
}
