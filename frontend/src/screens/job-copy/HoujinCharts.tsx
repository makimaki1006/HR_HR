import type { EChartsCoreOption } from "echarts/core";
import { EChart } from "../../components/EChart";
import type { CustomerData, CustomerDeal, Monthly, Point } from "./houjinModel";
import { hireAt, monthAdd, siteKey, sitesOf, sumKnown } from "./houjinModel";
import { yen } from "./homeModel";
const colors = ["#315b8a", "#78549e", "#394957", "#aa548c", "#287c99"];
export const chartOption = (axis: string[], series: object[], unit = "件") => ({
  animation: false,
  color: colors,
  tooltip: { trigger: "axis", confine: true },
  legend: { type: "scroll", top: 0 },
  grid: { left: 75, right: 25, top: 55, bottom: 48 },
  xAxis: {
    type: "category",
    data: axis,
    axisLabel: {
      hideOverlap: true,
      formatter: (value: string) =>
        /^\d{4}-\d{2}$/.test(value) ? value.slice(2) : value,
    },
  },
  yAxis: { type: "value", name: unit },
  series,
});
const point = (p: Point | null | undefined) =>
  p?.v == null
    ? null
    : {
        value: p.v,
        symbol: p.carry ? "emptyCircle" : "circle",
        symbolSize: p.carry ? 8 : 5,
      };
export function Figure({
  title,
  note,
  id,
  option,
  height = 240,
  onDealClick,
}: {
  title: string;
  note?: string;
  id: string;
  option: EChartsCoreOption;
  height?: number;
  onDealClick?: (id: string) => void;
}) {
  return (
    <figure className="hj-figure">
      <figcaption>{title}</figcaption>
      {note && <p className="jh-note">{note}</p>}
      <EChart
        option={option}
        testId={id}
        height={height}
        onReady={(instance) => {
          if (onDealClick)
            instance.on("click", (params) => {
              const data = params.data as { dealId?: string } | undefined;
              if (data?.dealId) onDealClick(data.dealId);
            });
        }}
      />
    </figure>
  );
}
/** 畳んだ見出しに出す図の数。下の各部品が出す Figure の数と同じ条件で数える */
export function monthlyFigureCount(mm: Monthly): number {
  const any = [...Object.values(mm.series), ...Object.values(mm.nps)].some(
    (ps) => ps.length > 0,
  );
  return any ? 3 : 0;
}
export function cpaFigureCount(
  data: CustomerData,
  deals: CustomerDeal[],
): number {
  const ids = new Set(deals.map((d) => d.deal_id));
  return (
    1 +
    data.cpa_by_site.filter((s) => s.points.some((p) => ids.has(p.deal_id)))
      .length
  );
}
export function acrossFigureCount(
  data: CustomerData,
  deals: CustomerDeal[],
): number {
  return 4 + cpaFigureCount(data, deals);
}
export function MonthlyCharts({
  mm,
  data,
}: {
  mm: Monthly;
  data: CustomerData;
}) {
  const months = [
    ...new Set(
      [...Object.values(mm.series), ...Object.values(mm.nps)].flatMap((ps) =>
        ps.map((p) => p.m),
      ),
    ),
  ].sort((a, b) => a - b);
  const axis = months.map((m) => monthAdd(mm.start, m));
  const line = (key: string, name: string) => ({
    name,
    type: "line",
    connectNulls: false,
    data: months.map((m) => point(mm.series[key]?.find((p) => p.m === m))),
  });
  const nps = months.map((m) =>
    Object.values(mm.nps)
      .flatMap((ps) => ps.filter((p) => p.m === m))
      .reduce<Point | undefined>(
        (hit, p) => (!hit || !p.carry ? p : hit),
        undefined,
      ),
  );
  const contacts = data.contacts.find((c) => c.deal_id === mm.deal_id);
  return (
    <article className="hj-monthly">
      <h4>{mm.name || "案件名未登録"}</h4>
      <p className="jh-note">
        横軸は暦の月。中空の点は前の記録の持ち越し。契約期間 {mm.period ?? "—"}
        か月・暦の月数 {mm.span_months ?? "—"}。
      </p>
      {months.length ? (
        <>
          <Figure
            title="応募・面接・採用"
            id={`hj-monthly-${mm.deal_id}`}
            option={chartOption(axis, [
              line("oubo", "応募"),
              line("mensetu", "面接"),
              line("syoudaku", "採用"),
            ])}
          />
          <div className="hj-chart-pair">
            <Figure
              title="定期NPS"
              id={`hj-nps-${mm.deal_id}`}
              height={190}
              option={chartOption(
                axis,
                [{ name: "定期NPS", type: "line", data: nps.map(point) }],
                "点",
              )}
            />
            <Figure
              title="接触"
              id={`hj-contact-${mm.deal_id}`}
              height={190}
              option={chartOption(axis, [
                {
                  name: "MTG・60秒超の通話",
                  type: "bar",
                  data: months.map((m) =>
                    contacts
                      ? contacts.dates.filter(
                          (d) => d.slice(0, 7) === monthAdd(mm.start, m),
                        ).length
                      : null,
                  ),
                },
              ])}
            />
          </div>
        </>
      ) : (
        <p>変更履歴なし</p>
      )}
    </article>
  );
}
export function CpaCharts({
  data,
  deals,
  prefix,
}: {
  data: CustomerData;
  deals: CustomerDeal[];
  prefix: string;
}) {
  const ids = new Set(deals.map((d) => d.deal_id));
  const rows = data.cpa3.filter((c) => ids.has(c.deal_id));
  return (
    <>
      <Figure
        title="採用単価の3つの出し方"
        note="総額÷採用数、月割り、同じ進捗の区分の中央値。稼働中・未確定は中空の点。"
        id={`${prefix}-cpa3`}
        option={chartOption(
          rows.map((r) => r.name),
          [
            {
              name: "総額÷採用数",
              type: "bar",
              data: rows.map((r) =>
                r.total === null
                  ? null
                  : {
                      value: r.total,
                      itemStyle:
                        r.active || r.censored
                          ? {
                              color: "#fff",
                              borderColor: colors[0],
                              borderWidth: 2,
                              borderType: "dashed",
                            }
                          : undefined,
                    },
              ),
            },
            {
              name: "月割り",
              type: "scatter",
              data: rows.map((r) => r.monthly),
            },
            {
              name: "同じ進捗の中央値",
              type: "scatter",
              symbol: "emptyCircle",
              data: rows.map((r) => r.band_median),
            },
          ],
          "円",
        )}
      />
      <div className="hj-scroll">
        <table aria-label="採用単価の3つの出し方">
          <thead>
            <tr>
              {[
                "案件",
                "採用",
                "総額÷採用数",
                "月割り",
                "進捗の区分",
                "区分の中央値",
                "状態",
              ].map((t) => (
                <th key={t}>{t}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.deal_id}>
                <th>{r.name}</th>
                <td>{r.syoudaku ?? "—"}</td>
                <td>{yen(r.total)}</td>
                <td>{yen(r.monthly)}</td>
                <td>{r.band ?? "—"}</td>
                <td>{yen(r.band_median)}</td>
                <td>{r.active || r.censored ? "未確定" : "確定"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {data.cpa_by_site
        .filter((s) => s.points.some((p) => ids.has(p.deal_id)))
        .map((s, i) => {
          const ps = s.points.filter((p) => ids.has(p.deal_id));
          return (
            <Figure
              key={s.site}
              title={`採用単価の推移・${deals.find((d) => d.site === s.site)?.site_name ?? s.site}`}
              note={ps
                .map(
                  (p) =>
                    `${p.start}：${yen(p.cpa)}・採用${String(p.syoudaku ?? "—")}人${p.censored || p.active ? "（未確定）" : ""}`,
                )
                .join(" ／ ")}
              id={`${prefix}-cpa-site-${String(i)}`}
              option={chartOption(
                ps.map((p) => p.start),
                [
                  {
                    name: "採用単価",
                    type: "line",
                    data: ps.map((p) =>
                      point({ m: 0, v: p.cpa, carry: p.censored || p.active }),
                    ),
                  },
                ],
                "円",
              )}
            />
          );
        })}
    </>
  );
}
export function AcrossCharts({
  data,
  deals,
}: {
  data: CustomerData;
  deals: CustomerDeal[];
}) {
  const ids = new Set(deals.map((d) => d.deal_id));
  const monthly = data.monthly.filter((m) => ids.has(m.deal_id));
  const recorded = monthly.filter((m) => (m.series.syoudaku ?? []).length);
  const months = [
    ...new Set(
      recorded.flatMap((m) =>
        (m.series.syoudaku ?? []).map((p) => monthAdd(m.start, p.m)),
      ),
    ),
  ].sort();
  const sites = sitesOf(deals);
  const totals = ["oubo", "mensetu", "syoudaku"] as const;
  const pair = (a: (typeof totals)[number], b: (typeof totals)[number]) => {
    const both = deals.filter((d) => d[a] !== null && d[b] !== null);
    const den = sumKnown(both, (d) => d[a]);
    return den
      ? `${(((sumKnown(both, (d) => d[b]) ?? 0) / den) * 100).toFixed(1)}%（${String(both.length)}件）`
      : "—";
  };
  const ltvMonths = [
    ...new Set(
      deals
        .filter((d) => d.start && d.amount !== null)
        .map((d) => d.start.slice(0, 7)),
    ),
  ].sort();
  return (
    <>
      <Figure
        title="応募→面接→採用の落ち方"
        note={`前段比は両方の値がある契約で計算。面接÷応募 ${pair("oubo", "mensetu")} ／ 採用÷面接 ${pair("mensetu", "syoudaku")}`}
        id="hj-funnel"
        option={chartOption(
          ["応募", "面接", "採用"],
          [
            {
              name: "合計",
              type: "bar",
              data: totals.map((k) => sumKnown(deals, (d) => d[k])),
            },
          ],
        )}
      />
      <CpaCharts data={data} deals={deals} prefix="hj-across" />
      <Figure
        title="案件ごとの採用数の推移"
        note={`採用の履歴 ${String(recorded.length)}件 ／ 選択 ${String(deals.length)}件。中空の点は持ち越し。`}
        id="hj-hires"
        option={chartOption(
          months,
          recorded.map((m) => ({
            name: m.name,
            type: "line",
            data: months.map((month) => point(hireAt(m, month))),
          })),
        )}
      />
      <Figure
        title="契約中の案件の採用数（累計）の合算"
        note="各月に契約期間中だった案件の累計を拠点ごとに積み上げ。持ち越しは薄い柱。"
        id="hj-hires-total"
        option={chartOption(
          months,
          sites.flatMap((s, i) =>
            [false, true].map((carry) => ({
              name: `${s.name}${carry ? "（持ち越し）" : ""}`,
              type: "bar",
              stack: "hires",
              itemStyle: {
                color: colors[i % colors.length],
                opacity: carry ? 0.4 : 1,
              },
              data: months.map((month) =>
                sumKnown(
                  recorded.filter((m) =>
                    s.rows.some((d) => d.deal_id === m.deal_id),
                  ),
                  (m) => {
                    const p = hireAt(m, month);
                    return !!p?.carry === carry ? (p?.v ?? null) : null;
                  },
                ),
              ),
            })),
          ),
        )}
      />
      <Figure
        title="LTVの推移"
        note="契約金額の累計を拠点ごとに積み上げ。"
        id="hj-ltv"
        option={chartOption(
          ltvMonths,
          sites.map((s) => ({
            name: s.name,
            type: "bar",
            stack: "ltv",
            data: ltvMonths.map((month) =>
              sumKnown(
                deals.filter(
                  (d) =>
                    siteKey(d) === s.key &&
                    !!d.start &&
                    d.start.slice(0, 7) <= month,
                ),
                (d) => d.amount,
              ),
            ),
          })),
          "円",
        )}
      />
    </>
  );
}

export function movementOption(data: CustomerData, deals: CustomerDeal[]) {
  const ids = new Set(deals.map((d) => d.deal_id));
  return {
    ...chartOption([], []),
    xAxis: {
      type: "value",
      scale: true,
      min: "dataMin",
      max: "dataMax",
      splitNumber: 4,
      axisLabel: {
        hideOverlap: true,
        formatter: (value: number) =>
          new Date(value).toLocaleDateString("ja-JP", {
            month: "numeric",
            day: "numeric",
          }),
      },
    },
    yAxis: {
      type: "category",
      data: [...deals.map((d) => d.name), "担当交代"],
      axisLabel: { width: 95, overflow: "truncate" },
    },
    grid: { left: 110, right: 20, top: 40, bottom: 40 },
    series: [
      {
        name: "接触",
        type: "scatter",
        data: data.contacts
          .filter((c) => ids.has(c.deal_id))
          .flatMap((c) =>
            c.dates.map((date) => ({
              value: [
                Date.parse(date),
                deals.findIndex((d) => d.deal_id === c.deal_id),
              ],
              dealId: c.deal_id,
            })),
          ),
      },
      {
        name: "担当交代",
        type: "scatter",
        symbol: "diamond",
        symbolSize: 12,
        data: data.handover
          .filter((h) => ids.has(h.deal_id))
          .map((h) => ({
            value: [Date.parse(h.date), deals.length],
            dealId: h.deal_id,
          })),
      },
    ],
  };
}
