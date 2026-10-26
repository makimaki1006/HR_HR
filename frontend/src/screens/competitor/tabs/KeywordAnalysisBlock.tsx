import type { KeywordAnalysis } from '../../../generated/KeywordAnalysis';
import type { TermShiftRow } from '../../../generated/TermShiftRow';
import { MultiLineChart } from '../charts/MultiLineChart';
import { fixed, fmtInt, MISSING } from '../format';

const COMPETITION: Record<string, string> = { HIGH: '強い', MEDIUM: '中程度', LOW: '弱い' };

const pct1 = (v: number | null): string => (v === null ? MISSING : `${fixed(v, 1)}%`);
const signed = (v: number): string => `${v >= 0 ? '+' : '-'}${fixed(Math.abs(v), 1)}`;

function ShiftTable({ heading, rows }: { heading: string; rows: TermShiftRow[] }) {
  return (
    <table className="cmp-table">
      <thead>
        <tr>
          <th>{heading}</th>
          <th>前の割合</th>
          <th>後の割合</th>
          <th>変化(ポイント)</th>
        </tr>
      </thead>
      <tbody>
        {rows.length === 0 && (
          <tr>
            <td colSpan={4}>該当する検索語がありません。</td>
          </tr>
        )}
        {rows.map((r) => (
          <tr key={r.term}>
            <td>{r.term}</td>
            <td className="cmp-num">{pct1(r.before_pct)}</td>
            <td className="cmp-num">{pct1(r.after_pct)}</td>
            <td className="cmp-num">{signed(r.diff_pt)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** 「キーワードの推移・分析」。4 つの部分は独立に欠けてよく、欠けたときは 1 文だけ出す。 */
export function KeywordAnalysisBlock({ data }: { data: KeywordAnalysis }) {
  const a = data.search_trend;
  const b = data.term_shifts;
  const c = data.pref_terms;
  const d = data.overlay;
  return (
    <div className="cmp-kw" id="competitor-keyword-analysis">
      <h3>キーワードの推移・分析</h3>

      <h4>Google検索数の推移</h4>
      {a.status === 'unavailable' ? (
        <p className="cmp-note">{a.message}</p>
      ) : (
        <>
          <p className="cmp-note">{a.scope}</p>
          <MultiLineChart
            label="Google検索数の推移"
            months={a.months}
            lines={a.series.map((s) => ({ label: s.label, values: s.values }))}
            unit="回/月"
          />
          <table className="cmp-table">
            <thead>
              <tr>
                <th>検索語</th>
                <th>直近の月</th>
                <th>検索数</th>
                <th>前年比</th>
                <th>ピーク月</th>
                <th>競合の強さ</th>
              </tr>
            </thead>
            <tbody>
              {a.series.map((s) => (
                <tr key={s.variant}>
                  <td>{s.label}</td>
                  <td>{s.latest_month || MISSING}</td>
                  <td className="cmp-num">{s.latest === null ? MISSING : fmtInt(s.latest)}</td>
                  <td className="cmp-num">{s.yoy_pct === null ? MISSING : `${signed(s.yoy_pct)}%`}</td>
                  <td>{s.peak_month === null ? MISSING : `${String(s.peak_month)}月`}</td>
                  <td>{COMPETITION[s.competition] ?? MISSING}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}

      <h4>Indeedの検索語の伸びと落ち</h4>
      {b.status === 'unavailable' ? (
        <p className="cmp-note">{b.message}</p>
      ) : (
        <>
          <p className="cmp-note">{b.scope}</p>
          {b.before_period !== '' && (
            <p className="cmp-note">
              {b.before_period}と{b.after_period}の割合を比べています。
            </p>
          )}
          <ShiftTable heading="伸びた検索語" rows={b.rising} />
          <ShiftTable heading="落ちた検索語" rows={b.falling} />
          <MultiLineChart
            label="検索語の割合の推移"
            months={b.months}
            lines={b.lines.map((l) => ({ label: l.term, values: l.values }))}
            unit="%"
          />
          <p className="cmp-note">
            検索語はIndeedが返す上位の語だけです。圏外に落ちた語は、減ったのか見えなくなったのか区別できません。
          </p>
          {b.thin_note !== null && <p className="cmp-note">{b.thin_note}</p>}
        </>
      )}

      <h4>選んだ都道府県の検索語</h4>
      {c.status === 'unavailable' ? (
        <p className="cmp-note">{c.message}</p>
      ) : (
        <>
          <p className="cmp-note">
            {c.prefecture}・{c.month}の上位の検索語。割合は一覧に載った語の合計に占める割合です。
          </p>
          {c.compare_month !== null && c.compare_kind !== null && (
            <p className="cmp-note">
              比べる月：{c.compare_month}（{c.compare_kind}）
            </p>
          )}
          <table className="cmp-table">
            <thead>
              <tr>
                <th>順位</th>
                <th>検索語</th>
                <th>クリック数</th>
                <th>割合</th>
                <th>比べる月の割合</th>
                <th>変化(ポイント)</th>
              </tr>
            </thead>
            <tbody>
              {c.rows.map((r) => (
                <tr key={r.term}>
                  <td className="cmp-num">{r.rank}</td>
                  <td>{r.term}</td>
                  <td className="cmp-num">{fmtInt(r.clicks)}</td>
                  <td className="cmp-num">{fixed(r.share_pct, 1)}%</td>
                  <td className="cmp-num">{r.out_of_range ? '圏外' : pct1(r.compare_share_pct)}</td>
                  <td className="cmp-num">
                    {r.out_of_range ? '圏外' : r.change_pt === null ? MISSING : signed(r.change_pt)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}

      <h4>Indeed閲覧数とGoogle検索数の重ね表示</h4>
      {d.status === 'unavailable' ? (
        <p className="cmp-note">{d.message}</p>
      ) : (
        <>
          <p className="cmp-note">{d.caption}</p>
          <p className="cmp-note">{d.base_month}を100とした指数です。</p>
          <MultiLineChart
            label="閲覧数と検索数の指数"
            months={d.months}
            lines={[
              { label: d.indeed_label, values: d.indeed_index },
              { label: d.google_label, values: d.google_index },
            ]}
            unit="指数"
          />
        </>
      )}

      {data.caveats.map((t) => (
        <p className="cmp-note" key={t}>
          {t}
        </p>
      ))}
    </div>
  );
}
