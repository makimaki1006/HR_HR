import { useEffect, useState } from "react";
import { apiGet } from "../../api/client";
import { ownerParams } from "./OwnerTodo";
import {
  updateJobCopyQuery,
  useJobCopySearch,
  navigateJobCopy,
} from "./navigation";
import { CompanyView } from "./CompanyView";
import { RiskDetail, detailRow, jobsHref } from "./ConsultantRisk";
import { yen } from "./homeModel";
import type { CustomerData, CustomerDeal, CustomerIndex } from "./houjinModel";
import {
  customerFilters,
  customerPresets,
  filterCustomers,
  filterKeys,
  presetQuery,
  sitesOf,
  sumKnown,
} from "./houjinModel";
import {
  AcrossCharts,
  CpaCharts,
  Figure,
  MonthlyCharts,
  movementOption,
  chartOption,
} from "./HoujinCharts";
import "./home.css";
import "./houjin.css";
export interface HoujinResponse {
  status: string;
  index: CustomerIndex[];
  default_houjin: string | null;
  selected_houjin: string | null;
  customer: CustomerData | null;
  companies: { id: string; name: string | null }[];
  linkage: { total: number; unmatched: number };
}
export function HoujinView() {
  const search = useJobCopySearch();
  const query = new URLSearchParams(search);
  const ownerQuery = ownerParams(search);
  const requested = query.get("houjin") ?? "";
  const [loaded, setLoaded] = useState<{
    scope: string;
    value: HoujinResponse;
  } | null>(null);
  const [extraDeal, setExtraDeal] = useState<{
    id: string;
    name: string;
    scope: string;
  } | null>(null);
  const [error, setError] = useState(false);
  const [reload, setReload] = useState(0);
  const scope = `${ownerQuery}&houjin=${encodeURIComponent(requested)}`;
  useEffect(() => {
    const ctl = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    const load = async () => {
      const r = await apiGet<HoujinResponse>(`/api/job-copy/houjin?${scope}`, {
        signal: ctl.signal,
        timeoutMs: 120_000,
      });
      if (ctl.signal.aborted) return;
      if (!r.ok) {
        setError(true);
        return;
      }
      setError(false);
      setLoaded({ scope, value: r.data });
      if (r.data.status === "preparing")
        timer = setTimeout(() => {
          void load();
        }, 2000);
    };
    void load();
    return () => {
      ctl.abort();
      clearTimeout(timer);
    };
  }, [scope, reload]);
  const ready =
    loaded?.scope === scope && loaded.value.status === "ready"
      ? loaded.value
      : null;
  const rows = filterCustomers(ready?.index ?? [], query);
  const matchesSelection = rows.some((r) => r.houjin === ready?.selected_houjin);
  const firstMatch = rows[0]?.houjin;
  useEffect(() => {
    if (ready && !matchesSelection && firstMatch) {
      updateJobCopyQuery({ houjin: firstMatch, houjin_site: null, risk_deal: null }, "replace");
    }
  }, [ready, matchesSelection, firstMatch]);
  const data = matchesSelection ? ready?.customer : null;
  const deals = data?.deals ?? [];
  const [selection, setSelection] = useState<{
    houjin: string;
    excluded: string[];
  }>({ houjin: "", excluded: [] });
  const excluded =
    selection.houjin === ready?.selected_houjin ? selection.excluded : [];
  const toggle = (id: string) => {
    setSelection({
      houjin: ready?.selected_houjin ?? "",
      excluded: excluded.includes(id)
        ? excluded.filter((x) => x !== id)
        : [...excluded, id],
    });
  };
  const sites = sitesOf(deals);
  const site =
    sites.find((s) => s.key === query.get("houjin_site")) ??
    [...sites].sort((a, b) => b.lastStart.localeCompare(a.lastStart))[0];
  const siteDeals = site?.rows ?? [];
  const siteIds = new Set(siteDeals.map((d) => d.deal_id));
  const selected =
    deals.find((d) => d.deal_id === query.get("risk_deal")) ??
    (matchesSelection && extraDeal?.scope === scope && extraDeal.id === query.get("risk_deal")
      ? { deal_id: extraDeal.id, name: extraDeal.name }
      : null);
  const choose = (houjin: string) => {
    updateJobCopyQuery({ houjin, houjin_site: null, risk_deal: null });
  };
  const open = (id: string) => {
    updateJobCopyQuery({ risk_deal: id });
  };
  const dealTable = (items: CustomerDeal[], label = "契約の系列") => (
    <div className="hj-scroll">
      <table aria-label={label}>
        <thead>
          <tr>
            {[
              "取引",
              "ステージ",
              "拠点",
              "種別",
              "開始〜満了",
              "継続",
              "金額",
              "応募",
              "面接",
              "採用",
              "状態",
            ].map((t) => (
              <th key={t}>{t}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {items.map((d) => (
            <tr
              key={d.deal_id}
              onClick={(e) => {
                if (!(e.target as HTMLElement).closest("button,a"))
                  open(d.deal_id);
              }}
            >
              <th>
                <button
                  className="jh-link"
                  onClick={() => {
                    open(d.deal_id);
                  }}
                >
                  {d.name || "案件名未登録"}
                </button>
              </th>
              <td>{d.stage}</td>
              <td>{d.site_name || "拠点未登録"}</td>
              <td>{d.kind}</td>
              <td>
                {d.start || "—"}
                <br />〜{d.expiration || "—"}
              </td>
              <td>{d.renewal_no ?? "—"}</td>
              <td>{yen(d.amount)}</td>
              <td>{d.oubo ?? "—"}</td>
              <td>{d.mensetu ?? "—"}</td>
              <td>{d.syoudaku ?? "—"}</td>
              <td>
                {d.is_active ? "稼働中" : "終了"}
                {d.right_censored && "・未確定"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
  return (
    <div className="jc-page">
      <main className="jh-home hj-main" id="job-details">
        <header className="jh-head">
          <h1>法人</h1>
          <button
            className="jc-button"
            onClick={() => {
              setReload((n) => n + 1);
            }}
          >
            再読み込み
          </button>
        </header>
        {error && (
          <p role="alert">
            法人を読み込めませんでした。再読み込みしてください。
          </p>
        )}
        {!ready && !error && (
          <p role="status">
            {loaded?.value.status === "failed"
              ? "法人を取得できませんでした。"
              : "法人を読み込んでいます…"}
          </p>
        )}
        {ready && (
          <>
            <section aria-label="法人の選択" className="hj-picker">
              <div className="hj-controls">
                <label>
                  法人を選ぶ
                  <select
                    value={matchesSelection ? ready.selected_houjin ?? "" : ""}
                    onChange={(e) => {
                      choose(e.target.value);
                    }}
                  >
                    <option value="">法人を選ぶ</option>
                    {rows.map((r) => (
                      <option key={r.houjin} value={r.houjin}>
                        {r.name}（取引{r.deals ?? "—"} / 拠点{r.sites ?? "—"}）
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  法人の検索
                  <input
                    value={query.get("houjin_search") ?? ""}
                    onChange={(e) => {
                      updateJobCopyQuery(
                        { houjin_search: e.target.value || null },
                        "replace",
                      );
                    }}
                  />
                </label>
                <label>
                  並び
                  <select
                    value={query.get("houjin_sort") ?? "ltv"}
                    onChange={(e) => {
                      updateJobCopyQuery(
                        { houjin_sort: e.target.value },
                        "replace",
                      );
                    }}
                  >
                    <option value="ltv">LTVが大きい順</option>
                    <option value="deals">取引が多い順</option>
                    <option value="name">法人名順</option>
                  </select>
                </label>
              </div>
              <div className="hj-controls">
                {customerPresets.map((p) => (
                  <button
                    key={p.label}
                    className="jc-button"
                    aria-pressed={filterKeys.every(
                      (k) =>
                        (query.get(`houjin_${k}`) ?? "") ===
                        (p.values[k] ?? ""),
                    )}
                    onClick={() => {
                      updateJobCopyQuery(presetQuery(p.values), "replace");
                    }}
                  >
                    {p.label}
                  </button>
                ))}
                <button
                  className="jc-button"
                  onClick={() => {
                    updateJobCopyQuery(
                      { ...presetQuery({}), houjin_search: null },
                      "replace",
                    );
                  }}
                >
                  条件を戻す
                </button>
              </div>
              <details>
                <summary>絞り込み</summary>
                <p className="jh-note">
                  月額は取引の最大額、継続の回数は最大回数。空欄の条件は使いません。
                </p>
                <div className="hj-controls">
                  {customerFilters.map(([key, , label, unit]) => (
                    <label key={key}>
                      {label}
                      <input
                        type="number"
                        min="0"
                        step="any"
                        aria-label={`${label}の下限`}
                        value={query.get(`houjin_${key}`) ?? ""}
                        onChange={(e) => {
                          updateJobCopyQuery(
                            { [`houjin_${key}`]: e.target.value || null },
                            "replace",
                          );
                        }}
                      />
                      {unit}以上
                      {key === "sites" && (
                        <>
                          <input
                            type="number"
                            min="0"
                            aria-label="拠点数の上限"
                            value={query.get("houjin_sites_max") ?? ""}
                            onChange={(e) => {
                              updateJobCopyQuery(
                                { houjin_sites_max: e.target.value || null },
                                "replace",
                              );
                            }}
                          />
                          拠点以下
                        </>
                      )}
                    </label>
                  ))}
                  {(
                    [
                      ["active", "稼働中だけ"],
                      ["focus", "重点顧客だけ"],
                    ] as const
                  ).map(([key, label]) => (
                    <label key={key}>
                      <input
                        type="checkbox"
                        checked={query.get(`houjin_${key}`) === "1"}
                        onChange={(e) => {
                          updateJobCopyQuery(
                            {
                              [`houjin_${key}`]: e.target.checked ? "1" : null,
                            },
                            "replace",
                          );
                        }}
                      />
                      {label}
                    </label>
                  ))}
                </div>
              </details>
              <p role="status">
                条件に合う {rows.length}社 ／ 全{ready.index.length}社
              </p>
              {customerFilters.map(([key, field, label]) =>
                query.get(`houjin_${key}`) &&
                ready.index.some((r) => r[field] === null) ? (
                  <p key={key}>
                    {label}が未取得：
                    {ready.index.filter((r) => r[field] === null).length}社
                  </p>
                ) : null,
              )}
              <details>
                <summary>法人の一覧</summary>
                <div className="hj-scroll">
                  <table aria-label="法人の一覧">
                    <thead>
                      <tr>
                        {[
                          "法人",
                          "取引",
                          "拠点",
                          "従業員数",
                          "月額（最大）",
                          "LTV",
                          "継続（最大）",
                          "応募",
                          "面接",
                          "承諾",
                          "稼働中",
                          "最終満了",
                        ].map((t) => (
                          <th key={t}>{t}</th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {rows.map((r) => (
                        <tr key={r.houjin}>
                          <th>
                            <button
                              className="jh-link"
                              onClick={() => {
                                choose(r.houjin);
                              }}
                            >
                              {r.name}
                            </button>
                          </th>
                          <td>{r.deals ?? "—"}</td>
                          <td>{r.sites ?? "—"}</td>
                          <td>{r.kigyouzentaininzuu ?? "—"}</td>
                          <td>{yen(r.monthly_amount_max)}</td>
                          <td>{yen(r.ltv)}</td>
                          <td>{r.max_renewal_no ?? "—"}</td>
                          <td>{r.oubo_total ?? "—"}</td>
                          <td>{r.mensetu_total ?? "—"}</td>
                          <td>{r.syoudaku_total ?? "—"}</td>
                          <td>{r.active ?? "—"}</td>
                          <td>{r.last_expiration || "—"}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </details>
            </section>
            {!rows.length && <p role="status">条件に合う法人がありません</p>}
            {data?.customer && (
              <>
                <section aria-label="法人の数字">
                  <h2>{data.customer.name}</h2>
                  <div className="hj-kpis">
                    {[
                      ["取引", `${String(deals.length)}件`],
                      [
                        "稼働中",
                        `${String(deals.filter((d) => d.is_active).length)}件`,
                      ],
                      ["拠点", `${String(data.customer.sites ?? "—")}拠点`],
                      ["LTV", yen(sumKnown(deals, (d) => d.amount))],
                      [
                        "継続回数（最大）",
                        String(data.customer.max_renewal_no ?? "—"),
                      ],
                      [
                        "最終満了",
                        deals.reduce(
                          (m, d) => (d.expiration > m ? d.expiration : m),
                          "",
                        ) || "—",
                      ],
                    ].map(([label, value]) => (
                      <div key={label}>
                        <span>{label}</span>
                        <strong>{value}</strong>
                      </div>
                    ))}
                  </div>
                  <p className="jh-note">LTVは追加契約を除く契約金額の合計。</p>
                  {data.customer.last_expiration !==
                    deals
                      .map((d) => d.expiration)
                      .sort()
                      .at(-1) && (
                    <p className="jh-note">
                      追加契約を含む最終満了は{" "}
                      {data.customer.last_expiration || "未取得"}。
                    </p>
                  )}
                  {data.customer.ltv !== sumKnown(deals, (d) => d.amount) && (
                    <p className="jh-note">
                      一覧のLTVは追加契約を含む {yen(data.customer.ltv)}。
                    </p>
                  )}
                </section>
                <section>
                  <h2>拠点の比較</h2>
                  <p className="jh-note">
                    金額と採用は値がある契約の合計。括弧内は採用の値がある契約数。
                  </p>
                  <div className="hj-scroll">
                    <table aria-label="拠点の比較">
                      <thead>
                        <tr>
                          {[
                            "拠点",
                            "契約",
                            "稼働中",
                            "継続（最大）",
                            "金額の合計",
                            "採用の合計",
                            "最初の開始",
                            "最後の満了",
                          ].map((t) => (
                            <th key={t}>{t}</th>
                          ))}
                        </tr>
                      </thead>
                      <tbody>
                        {sites.map((s) => (
                          <tr key={s.key}>
                            <th>
                              <button
                                className="jh-link"
                                onClick={() => {
                                  updateJobCopyQuery({ houjin_site: s.key });
                                }}
                              >
                                {s.name}
                              </button>
                            </th>
                            <td>{s.rows.length}</td>
                            <td>{s.rows.filter((d) => d.is_active).length}</td>
                            <td>
                              {Math.max(
                                0,
                                ...s.rows.map((d) => d.renewal_no ?? 0),
                              )}
                            </td>
                            <td>{yen(sumKnown(s.rows, (d) => d.amount))}</td>
                            <td>
                              {sumKnown(s.rows, (d) => d.syoudaku) ?? "—"}（
                              {s.rows.filter((d) => d.syoudaku !== null).length}
                              件）
                            </td>
                            <td>
                              {s.rows
                                .map((d) => d.start)
                                .filter(Boolean)
                                .sort()[0] ?? "—"}
                            </td>
                            <td>
                              {s.rows
                                .map((d) => d.expiration)
                                .filter(Boolean)
                                .sort()
                                .at(-1) ?? "—"}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                </section>
                <section id="hj-site">
                  <h2>事業所</h2>
                  <label>
                    拠点を選ぶ
                    <select
                      value={site?.key ?? ""}
                      onChange={(e) => {
                        updateJobCopyQuery({ houjin_site: e.target.value });
                      }}
                    >
                      {sites.map((s) => (
                        <option key={s.key} value={s.key}>
                          {s.name}（契約{s.rows.length}件）
                        </option>
                      ))}
                    </select>
                  </label>
                  <p>
                    法人の{deals.length}件のうち、この拠点の{siteDeals.length}件
                    {!query.get("houjin_site") && "（最新の契約がある拠点）"}
                  </p>
                  <h3>契約の系列</h3>
                  {dealTable(siteDeals)}
                  <Figure
                    title="契約の連なり"
                    id="hj-contracts"
                    onDealClick={open}
                    note="案件ごとの開始日と満了日。"
                    option={{
                      ...chartOption([], []),
                      xAxis: {
                        type: "time",
                        axisLabel: {
                          formatter: (value: number) =>
                            new Date(value).toLocaleDateString("ja-JP", {
                              year: "numeric",
                              month: "numeric",
                            }),
                        },
                      },
                      yAxis: {
                        type: "category",
                        data: siteDeals.map((d) => d.name),
                      },
                      series: siteDeals.map((d, i) => ({
                        name: d.name,
                        type: "line",
                        data: [
                          { value: [d.start, i], dealId: d.deal_id },
                          { value: [d.expiration, i], dealId: d.deal_id },
                        ],
                        symbolSize: 8,
                      })),
                    }}
                  />
                  <div className="hj-site-details" id="hj-site-details">
                    <div>
                      {" "}
                      <h3>応募・面接・採用の推移</h3>
                      {data.monthly
                        .filter((m) => siteIds.has(m.deal_id))
                        .map((mm) => (
                          <MonthlyCharts key={mm.deal_id} mm={mm} data={data} />
                        ))}
                    </div>
                    <aside>
                      {" "}
                      <h3>こちらの動き</h3>
                      <p className="jh-note">接触はMTGと60秒超の通話。</p>
                      <div className="hj-scroll">
                        <table aria-label="接触と担当交代">
                          <thead>
                            <tr>
                              <th>案件</th>
                              <th>接触日</th>
                              <th>担当の交代</th>
                            </tr>
                          </thead>
                          <tbody>
                            {siteDeals.map((d) => (
                              <tr key={d.deal_id}>
                                <th>{d.name}</th>
                                <td>
                                  {data.contacts
                                    .find((c) => c.deal_id === d.deal_id)
                                    ?.dates.join("、") ?? "記録なし"}
                                </td>
                                <td>
                                  {data.handover
                                    .filter((h) => h.deal_id === d.deal_id)
                                    .map(
                                      (h) =>
                                        `${h.date} ${h.from} → ${h.to}（${h.reflected || "反映状況未取得"}）`,
                                    )
                                    .join("、") || "記録なし"}
                                </td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                      <Figure
                        title="接触と担当交代の推移"
                        id="hj-movement"
                        height={190}
                        option={movementOption(data, siteDeals)}
                        onDealClick={open}
                      />
                      <h3>MTGの履歴</h3>
                      <div className="hj-scroll">
                        <table aria-label="MTGの履歴" className="hj-mtg-table">
                          <thead>
                            <tr>
                              {[
                                "開催日",
                                "案件",
                                "リスク判定",
                                "やること",
                                "懸念",
                                "前向きな話",
                                "次回予定",
                                "抽出",
                              ].map((t) => (
                                <th key={t}>{t}</th>
                              ))}
                            </tr>
                          </thead>
                          <tbody>
                            {data.mtgs
                              .filter((m) => siteIds.has(m.deal_id))
                              .map((m, i) => (
                                <tr key={i}>
                                  <td>{m.date}</td>
                                  <th>
                                    <button
                                      className="jh-link"
                                      onClick={() => {
                                        open(m.deal_id);
                                      }}
                                    >
                                      {
                                        deals.find(
                                          (d) => d.deal_id === m.deal_id,
                                        )?.name
                                      }
                                    </button>
                                  </th>
                                  <td>{m.risk || "—"}</td>
                                  <td>{m.todo || "—"}</td>
                                  <td>{m.concern || "—"}</td>
                                  <td>{m.positive || "—"}</td>
                                  <td>{m.next || "—"}</td>
                                  <td>{m.extracted ? "済" : "未"}</td>
                                </tr>
                              ))}
                          </tbody>
                        </table>
                      </div>
                    </aside>
                  </div>{" "}
                  <h3>採用単価の推移</h3>
                  <CpaCharts data={data} deals={siteDeals} prefix="hj-site" />
                </section>
                <section>
                  <h2>拠点をまたいだ図</h2>
                  <h3>図に入れる案件</h3>
                  <div className="hj-controls">
                    <button
                      className="jc-button"
                      onClick={() => {
                        setSelection({
                          houjin: ready.selected_houjin ?? "",
                          excluded: [],
                        });
                      }}
                    >
                      全部選ぶ
                    </button>
                    <button
                      className="jc-button"
                      onClick={() => {
                        setSelection({
                          houjin: ready.selected_houjin ?? "",
                          excluded: deals.map((d) => d.deal_id),
                        });
                      }}
                    >
                      全部外す
                    </button>
                  </div>
                  <div className="hj-picks">
                    {deals.map((d) => (
                      <label key={d.deal_id}>
                        <input
                          type="checkbox"
                          checked={!excluded.includes(d.deal_id)}
                          onChange={() => {
                            toggle(d.deal_id);
                          }}
                        />
                        {d.name}・{d.site_name}・{yen(d.amount)}
                      </label>
                    ))}
                  </div>
                  <p>
                    全{deals.length}件中{deals.length - excluded.length}
                    件を図に表示
                  </p>
                  <AcrossCharts
                    data={data}
                    deals={deals.filter((d) => !excluded.includes(d.deal_id))}
                  />
                </section>
                <section>
                  <h2>重点顧客の内訳</h2>
                  <Figure
                    title="全担当の重点顧客"
                    id="hj-focus"
                    height={210}
                    option={{
                      animation: false,
                      tooltip: { trigger: "item" },
                      legend: { type: "scroll", top: 0 },
                      series: [
                        {
                          type: "pie",
                          radius: ["35%", "60%"],
                          center: ["50%", "60%"],
                          label: { formatter: "{b}：{c}社" },
                          data: [
                            { name: "重点顧客", value: data.focus.n_focus },
                            {
                              name: "その他",
                              value: data.focus.n_display - data.focus.n_focus,
                            },
                          ],
                        },
                      ],
                    }}
                  />
                  <p>
                    {data.customer.focus ? "重点顧客" : "通常の顧客"}：
                    {data.customer.focus_why.join(" ／ ") || "該当する条件なし"}
                  </p>
                  <p>
                    表示対象 {data.focus.n_display}社中 {data.focus.n_focus}社
                    ／ 全{data.focus.n_houjin}法人中 {data.focus.n_focus_all}社
                  </p>
                  <p>
                    月額30万円以上 {data.focus.monthly_over_300k}社 ／
                    従業員1,000人以上 {data.focus.enterprise}社 ／ 拠点3つ以上{" "}
                    {data.focus.multi_site}社
                  </p>
                  <p className="jh-note">
                    条件は重複。表示対象には追加契約のみの
                    {data.focus.n_display_option_only}
                    社を含み、全法人の集計は追加契約のみの
                    {data.focus.n_houjin_option_only}社を除く。
                  </p>
                </section>
                <section>
                  <h2>取引と求人</h2>
                  <a href="/consulting#monthly/results?at=rs-hq">
                    他の法人との比較（コンサルKPI）
                  </a>
                  {dealTable(deals, "取引と求人の取引")}
                  {ready.companies.map((company) => (
                    <div key={company.id}>
                      {/^\d+$/.test(data.meta.hubspot_portal_id ?? "") && (
                        <a
                          href={`https://app.hubspot.com/contacts/${data.meta.hubspot_portal_id ?? ""}/record/0-2/${company.id}`}
                          target="_blank"
                          rel="noreferrer"
                        >
                          HubSpotで会社を開く
                        </a>
                      )}
                      <CompanyView
                        embedded
                        company={{
                          id: company.id,
                          name: company.name ?? data.customer?.name ?? "",
                        }}
                        billingByJob={{}}
                        onName={Boolean}
                        onOpenDeal={(d) => {
                          setExtraDeal({ ...d, scope });
                          open(d.id);
                        }}
                        onOpenListing={(job) => {
                          navigateJobCopy(
                            new URL(
                              jobsHref(
                                search,
                                job.listingDealIds?.[0] ?? "",
                                job.hubspotId ?? job.id,
                              ),
                              window.location.origin,
                            ),
                          );
                        }}
                      />
                    </div>
                  ))}
                  {!ready.companies.length && (
                    <p>結び付く会社が見つかりませんでした。</p>
                  )}
                </section>
                <p className="jh-note">
                  集計日 {data.meta.today} ／ 取得元{" "}
                  {data.meta.source_as_of ?? "未取得"} ／ 取引 {deals.length}
                  件・稼働中 {deals.filter((d) => d.is_active).length}件
                </p>
              </>
            )}
            {rows.length > 0 && !data && <p role="status">法人を読み込んでいます…</p>}
            <p className="jh-note">
              会社との結び付き：{ready.linkage.total}法人中{" "}
              {ready.linkage.unmatched}法人が未確認
            </p>
            {selected && (
              <RiskDetail
                key={selected.deal_id}
                row={detailRow(
                  selected.deal_id,
                  selected.name,
                  data?.customer?.name,
                )}
                onClose={() => {
                  updateJobCopyQuery({ risk_deal: null });
                }}
                onSaved={() => {
                  setReload((n) => n + 1);
                }}
              />
            )}
          </>
        )}
      </main>
    </div>
  );
}
