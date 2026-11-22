// /app/listing-check: 求人チェック。HRハッカーに掲載中の求人の、給与の入力の誤り・最低賃金・記載の誤りを一覧にする。
// データは GET /api/listing-check/salary（案件管理と同じ条件の人にだけ返る。読み取りのみ）。
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  apiGet,
  ApiHttpError,
  AuthRequiredError,
  type ApiError,
  type ApiResult,
} from "../../api/client";
import type { SalaryCheckFinding } from "../../generated/SalaryCheckFinding";
import type { SalaryCheckResponse } from "../../generated/SalaryCheckResponse";

/** 全件を読むので、最初の 1 回は時間がかかる。 */
const LOAD_TIMEOUT_MS = 180_000;

function fetchCheck(
  fresh: boolean,
  signal?: AbortSignal,
): Promise<ApiResult<SalaryCheckResponse>> {
  return apiGet<SalaryCheckResponse>(
    `/api/listing-check/salary${fresh ? "?fresh=1" : ""}`,
    {
      timeoutMs: LOAD_TIMEOUT_MS,
      ...(signal ? { signal } : {}),
    },
  );
}

type Tier = "strong" | "mismatch" | "text" | "unknown";

export function tierOf(severity: string): Tier {
  if (severity === "strong") return "strong";
  if (severity === "unknown") return "unknown";
  if (severity === "text") return "text";
  return "mismatch";
}

export interface ListingGroup {
  listingId: string;
  hrhId: string;
  hrhUrl: string | null;
  hubspotUrl: string;
  name: string;
  prefecture: string;
  items: SalaryCheckFinding[];
}

/** 指摘を求人ごとにまとめる（応答の並び＝強い順を保つ）。 */
export function groupByListing(findings: SalaryCheckFinding[]): ListingGroup[] {
  const byId = new Map<string, ListingGroup>();
  for (const f of findings) {
    let group = byId.get(f.listing_id);
    if (!group) {
      group = {
        listingId: f.listing_id,
        hrhId: f.hrh_id,
        hrhUrl: f.hrh_url,
        hubspotUrl: f.hubspot_url,
        name: f.name,
        prefecture: f.prefecture,
        items: [],
      };
      byId.set(f.listing_id, group);
    }
    group.items.push(f);
  }
  return [...byId.values()];
}

function formatTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const parts = new Intl.DateTimeFormat("ja-JP", {
    timeZone: "Asia/Tokyo",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).formatToParts(d);
  const get = (type: string): string =>
    parts.find((p) => p.type === type)?.value ?? "";
  return `${get("year")}/${get("month")}/${get("day")} ${get("hour")}:${get("minute")}`;
}

function errorMessage(error: ApiError): string {
  if (error instanceof AuthRequiredError)
    return "ログインの期限が切れました。ログインし直してください。";
  if (error instanceof ApiHttpError) {
    if (error.status === 403)
      return "会社の Google アカウントでログインすると見られます。";
    if (error.status === 503)
      return "HubSpot が混み合っています。少し待ってから、もう一度読み込んでください。";
  }
  return "求人を読み込めませんでした。少し待ってから、もう一度読み込んでください。";
}

function Rows({ groups, tier }: { groups: ListingGroup[]; tier: Tier }) {
  return (
    <table className={`lc-table lc-table-${tier}`}>
      <thead>
        <tr>
          <th scope="col">求人</th>
          <th scope="col">見つかったこと</th>
          <th scope="col">開く</th>
        </tr>
      </thead>
      <tbody>
        {groups.map((g) => (
          <tr key={g.listingId}>
            <td className="lc-listing">
              <div className="lc-name">
                {g.name === "" ? "（案件名なし）" : g.name}
              </div>
              <div className="lc-sub">
                {g.prefecture === "" ? "都道府県なし" : g.prefecture}
                <span aria-hidden="true">・</span>
                求人ID {g.hrhId}
              </div>
            </td>
            <td>
              <ul className="lc-items">
                {g.items.map((f, i) => (
                  <li key={`${f.category}-${String(i)}`}>
                    <span className={`lc-tag lc-tag-${tierOf(f.severity)}`}>
                      {f.category_label}
                    </span>
                    <span className="lc-detail">{f.detail}</span>
                  </li>
                ))}
              </ul>
            </td>
            <td className="lc-links">
              {g.hrhUrl !== null && (
                <a href={g.hrhUrl} target="_blank" rel="noreferrer">
                  HRハッカー
                </a>
              )}
              <a href={g.hubspotUrl} target="_blank" rel="noreferrer">
                HubSpot
              </a>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function ListingCheckScreen() {
  const [data, setData] = useState<SalaryCheckResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [category, setCategory] = useState<string>("");

  const apply = useCallback((result: ApiResult<SalaryCheckResponse>) => {
    if (result.ok) setData(result.data);
    else setError(errorMessage(result.error));
    setLoading(false);
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    void fetchCheck(false, controller.signal).then((result) => {
      if (!controller.signal.aborted) apply(result);
    });
    return () => {
      controller.abort();
    };
  }, [apply]);

  const reload = (): void => {
    setLoading(true);
    setError(null);
    void fetchCheck(true).then(apply);
  };

  const shown = useMemo(
    () =>
      data === null
        ? []
        : data.findings.filter(
            (f) => category === "" || f.category === category,
          ),
    [data, category],
  );
  const tiers = useMemo(() => {
    const pick = (tier: Tier): ListingGroup[] =>
      groupByListing(shown.filter((f) => tierOf(f.severity) === tier));
    return {
      strong: pick("strong"),
      mismatch: pick("mismatch"),
      text: pick("text"),
      unknown: pick("unknown"),
    };
  }, [shown]);
  const total = (tier: Tier): number =>
    data === null
      ? 0
      : data.counts
          .filter((c) => tierOf(c.severity) === tier)
          .reduce((sum, c) => sum + c.count, 0);

  return (
    <div className="lc-page">
      <div className="lc-sheet">
        <nav className="lc-nav" aria-label="求人チェック">
          <a href="/">← ダッシュボード</a>
        </nav>
        <header className="lc-header">
          <div>
            <h1>求人チェック</h1>
            <p className="lc-lead">
              HRハッカーに掲載中の求人の、給与の入力・最低賃金・記載の誤り
            </p>
          </div>
          <button
            type="button"
            className="lc-reload"
            disabled={loading}
            onClick={reload}
          >
            {loading ? "読み込み中…" : "最新にする"}
          </button>
        </header>
        {data !== null && (
          <p className="lc-meta">
            {formatTime(data.checked_at)} 時点
            <span aria-hidden="true">｜</span>
            HubSpot で公開中 {data.total_listings.toLocaleString("ja-JP")}{" "}
            件のうち指摘あり {data.flagged_listings.toLocaleString("ja-JP")} 件
            <span aria-hidden="true">｜</span>
            最低賃金：{data.min_wage_source}
          </p>
        )}

        {data !== null &&
          (data.ended_listings > 0 || data.unconfirmed_listings > 0) && (
            <p className="lc-meta">
              {data.ended_listings > 0 && (
                <>
                  求人ページが無くなっていた {data.ended_listings}{" "}
                  件は、掲載が終わったものとして除いています。
                </>
              )}
              {data.unconfirmed_listings > 0 && (
                <>
                  求人ページを開けず、掲載中かを確かめられなかった求人が{" "}
                  {data.unconfirmed_listings} 件含まれます。
                </>
              )}
            </p>
          )}

        {error !== null && (
          <p className="lc-error" role="alert">
            {error}
          </p>
        )}
        {data === null && loading && (
          <p className="lc-loading" role="status">
            掲載中の求人を読んでいます。1 分ほどかかることがあります。
          </p>
        )}

        {data !== null && (
          <>
            <section className="lc-summary" aria-label="件数">
              <div className="lc-figure lc-figure-strong">
                <span className="lc-figure-label">桁の誤り・最低賃金</span>
                <strong>{total("strong")}</strong>
                <span className="lc-figure-unit">件</span>
              </div>
              <div className="lc-figure lc-figure-mismatch">
                <span className="lc-figure-label">本文との食い違い</span>
                <strong>{total("mismatch")}</strong>
                <span className="lc-figure-unit">件</span>
              </div>
              <div className="lc-figure lc-figure-text">
                <span className="lc-figure-label">記載の誤り</span>
                <strong>{total("text")}</strong>
                <span className="lc-figure-unit">件</span>
              </div>
              <div className="lc-figure lc-figure-unknown">
                <span className="lc-figure-label">判断できないもの</span>
                <strong>{total("unknown")}</strong>
                <span className="lc-figure-unit">件</span>
              </div>
            </section>

            <div className="lc-filter" role="group" aria-label="区分で絞り込む">
              <button
                type="button"
                aria-pressed={category === ""}
                onClick={() => {
                  setCategory("");
                }}
              >
                すべて
              </button>
              {data.counts.map((c) => (
                <button
                  key={c.category}
                  type="button"
                  aria-pressed={category === c.category}
                  onClick={() => {
                    setCategory(c.category);
                  }}
                >
                  {c.label}
                  <span className="lc-filter-count">{c.count}</span>
                </button>
              ))}
            </div>

            {data.findings.length === 0 && (
              <p className="lc-empty">指摘はありません。</p>
            )}

            {tiers.strong.length > 0 && (
              <section className="lc-section lc-section-strong">
                <h2>桁の誤り・最低賃金</h2>
                <p className="lc-note">
                  額の桁が給与の種類と合わないもの、時給に直すと都道府県の最低賃金を下回るもの。
                </p>
                <Rows groups={tiers.strong} tier="strong" />
              </section>
            )}

            {tiers.mismatch.length > 0 && (
              <section className="lc-section lc-section-mismatch">
                <h2>本文との食い違い</h2>
                <p className="lc-note">
                  入力した額と、給与補足の文に書いた額が違うもの。
                </p>
                <Rows groups={tiers.mismatch} tier="mismatch" />
              </section>
            )}

            {tiers.text.length > 0 && (
              <section className="lc-section lc-section-text">
                <h2>記載の誤り</h2>
                <p className="lc-note">
                  書きかけの語、文字化け、金額の書き間違い、空の欄、題名・キャッチコピーと入力した給与の食い違い、年間休日の食い違い。
                </p>
                <Rows groups={tiers.text} tier="text" />
              </section>
            )}

            {tiers.unknown.length > 0 && (
              <details className="lc-section lc-section-unknown">
                <summary>
                  判断できないもの（
                  {tiers.unknown.reduce((sum, g) => sum + g.items.length, 0)}{" "}
                  件）
                </summary>
                <p className="lc-note">
                  働く時間や日数が分からず、最低賃金と比べられなかった求人。
                </p>
                <Rows groups={tiers.unknown} tier="unknown" />
              </details>
            )}
          </>
        )}
      </div>
    </div>
  );
}
