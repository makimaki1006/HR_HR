// /app/admin?view=hubspot: HubSpot 呼び出しの関所 (Rust の src/hubspot/gateway.rs) の観測値。
// データは GET /api/admin/hubspot-usage (管理者だけ。数字だけで、鍵やレコードの中身は無い)。15 秒ごとに読み直す。
import { useEffect, useState } from 'react';
import { ApiAbortedError, AuthRequiredError, apiGet } from '../../api/client';
import type { ApiError } from '../../api/client';
import type { HubSpotUsageResponse } from '../../generated/HubSpotUsageResponse';
import { redirectToLogin } from '../../shell/navigation';
import { describeApiError } from './useApiGet';

/** 読み直す間隔 */
export const HUBSPOT_USAGE_REFRESH_MS = 15_000;
export const HUBSPOT_USAGE_PATH = '/api/admin/hubspot-usage';

const NUM = new Intl.NumberFormat('ja-JP');
const num = (n: number): string => NUM.format(n);

/** 残り / 上限。どちらも分からなければ「未取得」 */
export function remainingText(remaining: number | null, max: number | null): string {
  if (remaining === null && max === null) return '未取得';
  return `残り ${remaining === null ? '?' : num(remaining)} / 上限 ${max === null ? '?' : num(max)}`;
}

/** ミリ秒を「1.2 秒」「350 ミリ秒」に。null は「記録なし」 */
export function durationText(ms: number | null): string {
  if (ms === null) return '記録なし';
  if (ms < 1000) return `${num(ms)} ミリ秒`;
  return `${(ms / 1000).toLocaleString('ja-JP', { maximumFractionDigits: 1 })} 秒`;
}

/** 当たりの割合 (当たり + 外れ が 0 なら「-」) */
export function hitRateText(hits: number, misses: number): string {
  const total = hits + misses;
  if (total === 0) return '-';
  return `${String(Math.round((hits / total) * 100))}%`;
}

function Kpi({ title, value, testId }: { title: string; value: string; testId: string }) {
  return (
    <div className="w8-kpi">
      <div className="w8-kpi-title">{title}</div>
      <div className="w8-kpi-value" style={{ fontSize: 20 }} data-testid={testId}>
        {value}
      </div>
    </div>
  );
}

export function HubSpotUsageView({ data }: { data: HubSpotUsageResponse }) {
  const r = data.rate_limit;
  const q = data.queue;
  const l = data.limits;
  return (
    <>
      <h1 className="w8-h1" style={{ marginBottom: 4 }}>
        HubSpot の利用状況
      </h1>
      <p className="w8-subtle">
        このアプリから HubSpot への呼び出しの状況です。HubSpot の鍵は社内の他の処理と共有しているため、「HubSpot 側の残り」はアカウント全体の値です。
        {HUBSPOT_USAGE_REFRESH_MS / 1000} 秒ごとに更新します。
      </p>
      {!data.configured && (
        <p className="w8-alert" role="status">
          HubSpot の鍵が設定されていません (CRM の画面からは HubSpot を呼びません)。
        </p>
      )}

      <h2 className="w8-section-title">HubSpot 側の残り（最後に受け取った応答）</h2>
      <div className="w8-kpis" data-testid="hubspot-rate-limit">
        <Kpi title="10 秒あたり" value={remainingText(r.per_10s_remaining, r.per_10s_max)} testId="rl-10s" />
        <Kpi title="1 秒あたり" value={remainingText(r.per_second_remaining, r.per_second_max)} testId="rl-1s" />
        <Kpi title="1 日あたり" value={remainingText(r.daily_remaining, r.daily_max)} testId="rl-daily" />
        <Kpi title="受け取った時刻" value={r.observed_at ?? '未取得'} testId="rl-at" />
      </div>

      <h2 className="w8-section-title">起動してからの回数</h2>
      <div className="w8-kpis" data-testid="hubspot-counters">
        <Kpi title="HubSpot への呼び出し" value={`${num(data.counters.calls)} 回`} testId="c-calls" />
        <Kpi title="うち検索" value={`${num(data.counters.search_calls)} 回`} testId="c-search" />
        <Kpi title="上限に達した (429)" value={`${num(data.counters.rate_limited)} 回`} testId="c-429" />
        <Kpi title="混雑で断った" value={`${num(data.counters.busy_rejected)} 回`} testId="c-busy" />
        <Kpi title="同じ読み取りをまとめた" value={`${num(data.counters.coalesced)} 回`} testId="c-coalesced" />
      </div>

      <h2 className="w8-section-title">待ち行列</h2>
      <div className="w8-kpis" data-testid="hubspot-queue">
        <Kpi title="順番待ちの呼び出し" value={`${num(q.waiting)} 件`} testId="q-waiting" />
        <Kpi title="順番待ちの検索" value={`${num(q.waiting_search)} 件`} testId="q-search" />
        <Kpi
          title={`待ち時間 中央値 / 95%（直近 ${String(q.wait_window_secs / 60)} 分・${num(q.wait_samples)} 件）`}
          value={`${durationText(q.wait_p50_ms)} / ${durationText(q.wait_p95_ms)}`}
          testId="q-wait"
        />
        <Kpi
          title="上限に達したための一時停止"
          value={q.paused_ms === null ? 'なし' : `停止中（あと ${durationText(q.paused_ms)}）`}
          testId="q-paused"
        />
      </div>

      <div className="w8-grid-2">
        <div>
          <h2 className="w8-section-title">種類別の呼び出し回数</h2>
          <div className="w8-table-wrap">
            <table className="w8-table" data-testid="hubspot-groups">
              <thead>
                <tr>
                  <th>種類</th>
                  <th className="w8-right">回数</th>
                </tr>
              </thead>
              <tbody>
                {data.calls_by_group.length === 0 ? (
                  <tr>
                    <td colSpan={2} className="w8-empty">
                      まだ呼び出していません
                    </td>
                  </tr>
                ) : (
                  data.calls_by_group.map((g) => (
                    <tr key={g.key}>
                      <td>{g.label}</td>
                      <td className="w8-right">{num(g.count)}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
        <div>
          <h2 className="w8-section-title">キャッシュ（HubSpot を呼ばずに済んだ割合）</h2>
          <div className="w8-table-wrap">
            <table className="w8-table" data-testid="hubspot-caches">
              <thead>
                <tr>
                  <th>対象（保持する時間）</th>
                  <th className="w8-right">使えた</th>
                  <th className="w8-right">読み直した</th>
                  <th className="w8-right">使えた割合</th>
                </tr>
              </thead>
              <tbody>
                {data.caches.length === 0 ? (
                  <tr>
                    <td colSpan={4} className="w8-empty">
                      まだ記録がありません
                    </td>
                  </tr>
                ) : (
                  data.caches.map((c) => (
                    <tr key={c.key}>
                      <td>{c.label}</td>
                      <td className="w8-right">{num(c.hits)}</td>
                      <td className="w8-right">{num(c.misses)}</td>
                      <td className="w8-right">{hitRateText(c.hits, c.misses)}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <p className="w8-note" data-testid="hubspot-limits">
        このアプリの上限: 1 秒あたり {num(l.per_second)} 回・10 秒あたり {num(l.per_10s)} 回（検索は{' '}
        {durationText(l.search_interval_ms)}に 1 回）。画面の操作は最大 {durationText(l.interactive_max_wait_ms)}、背景の取得は最大{' '}
        {durationText(l.background_max_wait_ms)}まで順番を待ち、それを超えるときは「混み合っています」と表示します。回数はサーバーを起動してからの合計です。
      </p>
    </>
  );
}

type PanelState =
  | { status: 'loading' }
  | { status: 'ok'; data: HubSpotUsageResponse; refreshError: ApiError | null }
  | { status: 'error'; error: ApiError };

/** 最初に 1 回読み、その後 15 秒ごとに読み直す。読み直しに失敗したら前の値を出したまま知らせる */
export function HubSpotUsagePanel({ intervalMs = HUBSPOT_USAGE_REFRESH_MS }: { intervalMs?: number }) {
  const [state, setState] = useState<PanelState>({ status: 'loading' });
  useEffect(() => {
    let controller: AbortController | null = null;
    let stopped = false;
    const load = () => {
      controller?.abort();
      const c = new AbortController();
      controller = c;
      void apiGet<HubSpotUsageResponse>(HUBSPOT_USAGE_PATH, { signal: c.signal, timeoutMs: 10_000 }).then((r) => {
        if (stopped || c.signal.aborted) return;
        if (!r.ok && r.error instanceof AuthRequiredError) {
          redirectToLogin();
          return;
        }
        if (r.ok) setState({ status: 'ok', data: r.data, refreshError: null });
        else if (!(r.error instanceof ApiAbortedError)) {
          const error = r.error;
          setState((prev) => (prev.status === 'ok' ? { ...prev, refreshError: error } : { status: 'error', error }));
        }
      });
    };
    load();
    const timer = window.setInterval(load, intervalMs);
    return () => {
      stopped = true;
      window.clearInterval(timer);
      controller?.abort();
    };
  }, [intervalMs]);

  if (state.status === 'loading') return <p data-testid="admin-loading">読み込み中…</p>;
  if (state.status === 'error') {
    const text = describeApiError(state.error);
    return (
      <div role="alert" className="w8-alert" data-testid="admin-error">
        <h1>{text.title}</h1>
        <p>{text.detail}</p>
      </div>
    );
  }
  return (
    <>
      {state.refreshError !== null && (
        <p className="w8-alert" role="alert" data-testid="hubspot-refresh-error">
          最新の値を読めませんでした。表示は前回の値のままです（{state.refreshError.message}）。
        </p>
      )}
      <HubSpotUsageView data={state.data} />
    </>
  );
}
