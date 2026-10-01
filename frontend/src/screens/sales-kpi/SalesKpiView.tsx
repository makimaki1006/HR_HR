// 営業KPI の見た目。状態は親 (SalesKpiScreen) が持ち、ここは props だけで描く
// (renderToStaticMarkup で値を検証できるように、描画中に DOM を触らない)。
// 文言・並び・色分けは旧画面 templates/tabs/sales_kpi.html と同じにする。
import { Fragment, useState, type ReactNode } from 'react';
import {
  ALL_TEAMS,
  CALL_PERIODS,
  KADEN_CLASSES,
  SHEETS,
  SNAP,
  actionView,
  ago,
  callsView,
  excludedParts,
  fmt,
  groupByDate,
  growText,
  growTone,
  hasKettei,
  hiddenCount,
  isSalesTeam,
  kadenListView,
  ketteiCell,
  ketteiView,
  lsGet,
  lsPct,
  matchedText,
  md,
  monthView,
  panelConf,
  pct,
  rangeText,
  scopeText,
  signed,
  snapView,
  stockOverview,
  tabsOf,
  teamLabel,
  teamOfMap,
  todayOf,
  visiblePeople,
  wd,
  weekDays,
  type AvgBase,
  type CardSpec,
  type DailyBar,
  type OpenKey,
  type Scope,
  type SnapMode,
  type StockOverview,
  type StockRow,
  type TabKey,
  type Wow,
} from './calc';
import type { CallPeriodKey, DealRow, Person, SalesKpiData } from './types';

export interface UiState {
  scope: Scope;
  openKey: OpenKey | null;
  dayKey: string | null;
  weekOpen: boolean;
  callPeriod: CallPeriodKey;
  snapMode: SnapMode;
  tab: TabKey;
  pickOpen: boolean;
}

export interface UiActions {
  setTeam: (team: string) => void;
  setPerson: (id: string) => void;
  /** ids を入れる (on=true) / 外す (on=false) */
  setHidden: (ids: readonly string[], on: boolean) => void;
  resetHidden: () => void;
  toggleOpen: (key: OpenKey) => void;
  closePanel: () => void;
  setDayKey: (dt: string | null) => void;
  setWeekOpen: (on: boolean) => void;
  setCallPeriod: (k: CallPeriodKey) => void;
  setSnapMode: (m: SnapMode) => void;
  setTab: (t: TabKey) => void;
  togglePick: () => void;
  toggleTheme: () => void;
}

export interface SalesKpiViewProps {
  data: SalesKpiData;
  ui: UiState;
  actions: UiActions;
}

const faint = { color: 'var(--faint)' } as const;
const sub = { color: 'var(--sub)' } as const;
const warn = { color: 'var(--warn)' } as const;
const alert = { color: 'var(--alert)' } as const;
const toneColor = (t: 'ok' | 'alert' | 'faint'): string => `var(--${t})`;

function SheetA({ k, label }: { k: keyof typeof SHEETS; label: string }) {
  return (
    <a href={SHEETS[k].url} target="_blank" rel="noopener">
      {label} ↗
    </a>
  );
}

// ---------------------------------------------------------------- カード

/** 前週比の 1 行 (旧 wowEl)。テストから旧 HTML と比べるので export。 */
export function WowLine({ w }: { w: Wow }) {
  if (w.same) {
    return (
      <div className="wow" style={faint}>
        先週と同じ
      </div>
    );
  }
  return (
    <div className="wow" style={{ color: w.good ? 'var(--ok)' : 'var(--alert)' }}>
      {w.arrow}
      {w.abs}
      {w.unit} <span style={faint}>先週 {w.prev}{w.unit}</span>
    </div>
  );
}

function Card({ o, open, onClick }: { o: CardSpec; open?: boolean; onClick?: () => void }) {
  const body = (
    <>
      <div className="lab">{o.lab}</div>
      <div className="v">
        {o.isPct ? pct(o.val) : fmt(o.val)}
        {o.unit ? <small>{o.unit}</small> : null}
      </div>
      {o.hint ? <div className="hint">{o.hint}</div> : null}
      {o.bpo ? (
        <div className="sub2">
          <span style={sub}>
            内 BPO {fmt(o.bpo.v)}件{o.bpo.ratio !== null ? `（${o.bpo.ratio}%）` : ''}
          </span>
        </div>
      ) : null}
      {o.sub2 ? (
        <div className="sub2">
          <span style={sub}>{o.sub2}</span>
        </div>
      ) : null}
      {o.avg ? (
        <div className="avg">
          {o.avg.label} {o.avg.value}
          <span style={{ opacity: 0.75 }}>（{o.avg.n}名）</span>
        </div>
      ) : null}
      {o.wow ? <WowLine w={o.wow} /> : null}
      {onClick ? <div className="open">{open ? '閉じる ▲' : '一覧を見る ▾'}</div> : null}
    </>
  );
  const cls = 'c' + (o.tone ? ' ' + o.tone : '');
  if (onClick) {
    return (
      <button type="button" className={cls} data-card={o.key} onClick={onClick}>
        {body}
      </button>
    );
  }
  return (
    <div className={cls} data-card={o.key}>
      {body}
    </div>
  );
}

// ---------------------------------------------------------------- 取引の行

function Item({ r, today, stale, days, done }: { r: DealRow; today: string; stale?: boolean; days?: boolean; done?: boolean }) {
  const subText = stale ? String(ago(today, r.date)) + '日前' : days ? String(r.days ?? '') + '日' : r.time || '';
  // 旧画面は r.url を href にしていたが、サーバの JSON に url は無い (旧画面でも undefined)。
  const url = (r as DealRow & { url?: string }).url;
  return (
    <a
      className={'item' + (stale ? ' stale' : '') + (done ? ' done' : '')}
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      data-deal={r.id}
    >
      <div className="d">
        <b>{r.date ? md(r.date) + '（' + wd(r.date) + '）' : '—'}</b>
        {subText ? <span>{subText}</span> : null}
      </div>
      <div className="nm">{r.name}</div>
      <div className="who">{r.ownerName}</div>
      <div className="go">HubSpotを開く ›</div>
    </a>
  );
}

function ListOf({ rows, today, opt }: { rows: readonly DealRow[]; today: string; opt: { stale?: boolean; days?: boolean; done?: boolean } }) {
  return (
    <div className="list">
      {rows.map((r) => (
        <Item key={r.id} r={r} today={today} stale={opt.stale ?? false} days={opt.days ?? false} done={opt.done ?? false} />
      ))}
    </div>
  );
}

// ---------------------------------------------------------------- 担当者を選ぶ

function PickPanel({ people, scope, actions }: { people: readonly Person[]; scope: Scope; actions: UiActions }) {
  const byTeam: Record<string, Person[]> = {};
  for (const p of people) (byTeam[p.team] ??= []).push(p);
  const order = Object.keys(byTeam).sort(
    (a, b) => Number(isSalesTeam(b)) - Number(isSalesTeam(a)) || a.localeCompare(b, 'ja'),
  );
  return (
    <div className="pick" id="pickpanel">
      <div className="ph">
        <b>数字に入れる担当者</b>
        <button type="button" className="close" onClick={actions.resetHidden}>
          全部戻す
        </button>
      </div>
      <p className="lead">
        チェックを外した人は、この画面の数字から抜けます。<b>あなたのブラウザにだけ残ります</b>
        （他の人の画面は変わりません）。
        <br />
        <span style={faint}>
          全員に効かせたいときは、<SheetA k="exclude" label="KPI営業_集計除外" /> を使ってください。
        </span>
      </p>
      {order.map((t) => {
        const list = (byTeam[t] ?? []).slice().sort((a, b) => a.name.localeCompare(b.name, 'ja'));
        const on = list.filter((p) => !scope.hidden.has(p.id)).length;
        return (
          <div className="grp2" key={t}>
            <div className="gh">
              <input
                type="checkbox"
                checked={on > 0}
                ref={(el) => {
                  if (el) el.indeterminate = on > 0 && on < list.length;
                }}
                onChange={(e) => {
                  actions.setHidden(
                    list.map((p) => p.id),
                    !e.currentTarget.checked,
                  );
                }}
              />
              <b>{t}</b>
              <span className="n">
                {on} / {list.length}名
              </span>
            </div>
            <div className="who2">
              {list.map((p) => (
                <label key={p.id} className={scope.hidden.has(p.id) ? 'off' : undefined}>
                  <input
                    type="checkbox"
                    checked={!scope.hidden.has(p.id)}
                    onChange={(e) => {
                      actions.setHidden([p.id], !e.currentTarget.checked);
                    }}
                  />
                  <span title={p.name + '（' + teamLabel(p) + '）'}>{p.name}</span>
                </label>
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------- いま手を打てること

function Panel({ data, ui, actions, av }: { data: SalesKpiData; ui: UiState; actions: UiActions; av: ReturnType<typeof actionView> }) {
  if (!ui.openKey) return <div className="panel hide" id="panel" />;
  const conf = panelConf(data, av, ui.openKey);
  const today = todayOf(data);
  let inner: ReactNode;
  if (!conf.rows.length) {
    inner = (
      <div className="empty">
        <b>該当はありません。</b>
      </div>
    );
  } else if (conf.byDay && conf.span) {
    const days = weekDays(conf.rows, conf.span, today);
    const byDay = groupByDate(conf.rows);
    const dayRows = ui.dayKey ? byDay[ui.dayKey] : undefined;
    inner = (
      <>
        <div className="wkstrip">
          {days.map((x) => (
            <button
              type="button"
              key={x.date}
              className={
                'wkday' +
                (x.rows.length ? '' : ' zero') +
                (x.past ? ' past' : '') +
                (x.today ? ' today' : '') +
                (ui.dayKey === x.date ? ' on' : '')
              }
              title={md(x.date) + '（' + wd(x.date) + '）の商談 ' + String(x.rows.length) + '件' + (x.rows.length ? '　押すと一覧が出ます' : '')}
              onClick={
                x.rows.length
                  ? () => {
                      actions.setDayKey(ui.dayKey === x.date ? null : x.date);
                    }
                  : undefined
              }
              data-date={x.date}
            >
              <div className="wd">{wd(x.date)}</div>
              <div className="dt">{md(x.date)}</div>
              <div className="n">
                {x.rows.length}
                <span className="u">件</span>
              </div>
            </button>
          ))}
        </div>
        {ui.dayKey && dayRows ? (
          <>
            <div className="day">
              {md(ui.dayKey)}（{wd(ui.dayKey)}）{'　'}{dayRows.length}件
            </div>
            <ListOf rows={dayRows} today={today} opt={{ done: !!dayRows[0]?.past }} />
            <button
              type="button"
              className="more"
              onClick={() => {
                actions.setDayKey(null);
              }}
            >
              この日を閉じる
            </button>
          </>
        ) : ui.weekOpen ? (
          <>
            {Object.keys(byDay)
              .sort()
              .map((dt) => {
                const rows = byDay[dt] ?? [];
                return (
                  <div key={dt}>
                    <div className="day">
                      {md(dt)}（{wd(dt)}）{'　'}{rows.length}件
                    </div>
                    <ListOf rows={rows} today={today} opt={{ done: !!rows[0]?.past }} />
                  </div>
                );
              })}
            <button
              type="button"
              className="more"
              onClick={() => {
                actions.setWeekOpen(false);
              }}
            >
              たたむ
            </button>
          </>
        ) : (
          <>
            <p className="wkhint">日を押すと、その日の商談が出ます。</p>
            <button
              type="button"
              className="more"
              onClick={() => {
                actions.setWeekOpen(true);
              }}
            >
              全部の日をまとめて見る ▾
            </button>
          </>
        )}
      </>
    );
  } else {
    inner = <ListOf rows={conf.rows} today={today} opt={conf.opt} />;
  }
  return (
    <div className="panel" id="panel">
      <div className="ph">
        <b>
          {conf.title}（{conf.rows.length}件）
        </b>
        <button type="button" className="close" onClick={actions.closePanel}>
          閉じる ✕
        </button>
      </div>
      <p className="lead">{conf.desc}</p>
      {inner}
    </div>
  );
}

// ---------------------------------------------------------------- 日別の架電グラフ (SVG)

interface Tip {
  date: string;
  calls: number;
  connected: number;
  long: number;
  x: number;
  y: number;
}

function DailyChart({ bars, mx }: { bars: readonly DailyBar[]; mx: number }) {
  const [tip, setTip] = useState<Tip | null>(null);
  const W = 880;
  const H = 150;
  const ml = 44;
  const mr = 10;
  const mt = 12;
  const mb = 28;
  const iw = W - ml - mr;
  const ih = H - mt - mb;
  const bw = iw / bars.length;
  const bar = Math.min(46, bw * 0.64);
  const ticks = [0, Math.round(mx / 2), mx];
  return (
    <>
      <div className="chartbox">
        <svg viewBox={`0 0 ${String(W)} ${String(H)}`} role="img" aria-label="日別の架電数" data-testid="daily-chart" data-bars={bars.length}>
          {ticks.map((v, i) => {
            const y = mt + ih - (ih * v) / mx;
            return (
              <g key={i}>
                <line x1={ml} x2={W - mr} y1={y} y2={y} stroke="var(--border)" strokeWidth={1} />
                <text x={ml - 6} y={y + 4} textAnchor="end" className="axis-l">
                  {fmt(v)}
                </text>
              </g>
            );
          })}
          {bars.map((x, ix) => {
            const cx = ml + bw * ix + bw / 2;
            const h = (ih * x.calls) / mx;
            const y = mt + ih - h;
            const hc = (ih * (x.connected || 0)) / mx;
            return (
              <g key={x.date} data-bar-date={x.date} data-calls={x.calls} data-connected={x.connected}>
                <g
                  tabIndex={0}
                  onPointerEnter={(e) => {
                    const r = e.currentTarget.getBoundingClientRect();
                    setTip({ ...x, x: r.left + r.width / 2, y: r.top });
                  }}
                  onPointerLeave={() => {
                    setTip(null);
                  }}
                  onFocus={(e) => {
                    const r = e.currentTarget.getBoundingClientRect();
                    setTip({ ...x, x: r.left + r.width / 2, y: r.top });
                  }}
                  onBlur={() => {
                    setTip(null);
                  }}
                >
                  <rect x={cx - bar / 2} y={y} width={bar} height={Math.max(h, 2)} rx={3} fill="var(--border)" />
                  <rect x={cx - bar / 2} y={mt + ih - hc} width={bar} height={Math.max(hc, 2)} rx={3} fill="var(--accent)" />
                </g>
                <text x={cx} y={H - 10} textAnchor="middle" className="axis-l">
                  {md(x.date)}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
      <div className="blegend" style={{ marginTop: 6 }}>
        <span>
          <i style={{ background: 'var(--accent)' }} />
          つながった
        </span>
        <span>
          <i style={{ background: 'var(--border)' }} />
          つながらず
        </span>
      </div>
      {tip ? (
        <div className="tip" style={{ opacity: 1, left: Math.max(8, tip.x - 130), top: Math.max(8, tip.y - 90) }}>
          <b>
            {md(tip.date)}（{wd(tip.date)}）
          </b>
          <div className="row">
            <span>架電</span>
            <span>{fmt(tip.calls)}件</span>
          </div>
          <div className="row">
            <span>つながった</span>
            <span>{fmt(tip.connected)}件</span>
          </div>
          <div className="row">
            <span>5分超</span>
            <span>{fmt(tip.long)}件</span>
          </div>
        </div>
      ) : null}
    </>
  );
}

// ---------------------------------------------------------------- 架電

function CallsSection({ data, ui, actions, teamOf, ab, hid }: { data: SalesKpiData; ui: UiState; actions: UiActions; teamOf: Record<string, string>; ab: AvgBase | null; hid: boolean }) {
  const cv = callsView(data, ui.scope, teamOf, ab, ui.callPeriod);
  const periodLabel = CALL_PERIODS.find((p) => p.key === ui.callPeriod)?.label ?? '';
  if (!cv) {
    return (
      <>
        <h2>架電</h2>
        <p className="lead" id="lead3">
          架電データがありません。
        </p>
      </>
    );
  }
  return (
    <>
      <h2>架電</h2>
      <p className="lead" id="lead3">
        Zoomのログから数えています。<b>架電数＝つながった通話</b>で、現場が数えている数と合わせています。
        {cv.fresh === 'none' ? (
          <>
            <br />
            <b style={warn}>この期間の架電はまだ集計されていません。</b>
            {cv.upto ? <span style={faint}>（集計済みは {md(cv.upto)} まで）</span> : null}
          </>
        ) : cv.fresh === 'partial' ? (
          <>
            <br />
            <span style={warn}>
              {md(cv.upto)} は{cv.asof ? cv.asof + ' 時点' : '集計中'}の数です。
            </span>
            <span style={faint}>その日の途中までしか入っていません。取り直すのは平日の 6:30 と 18:00 で、その間は数字が変わりません。</span>
          </>
        ) : null}
        <br />
        <span style={faint}>つながらなかった発信（相手が出る前に切った・失敗した）は「発信した回数」にだけ入ります。</span>
      </p>
      <div className="chiprow" id="callperiod" style={{ marginBottom: 11 }}>
        {CALL_PERIODS.map((p) => (
          <button
            type="button"
            key={p.key}
            className={'chip' + (p.key === ui.callPeriod ? ' on' : '')}
            onClick={() => {
              actions.setCallPeriod(p.key);
            }}
          >
            {p.label}
          </button>
        ))}
      </div>
      <div className="cards" id="cards3">
        {cv.cards.map((o) => (
          <Card key={o.key} o={o} />
        ))}
      </div>
      <div id="kadenbar">
        <div className="c">
          <div className="lab">
            日別の架電数（全社・担当者が紐づかない発信も含む）
            {hid ? (
              <span className="sm">
                <b style={warn}>チェックの絞り込みは効きません</b>（日ごとの合計しか持っていないため）
              </span>
            ) : null}
          </div>
          {cv.bars.length ? <DailyChart bars={cv.bars} mx={cv.mx} /> : null}
        </div>
        {cv.rows.length ? (
          <div className="c" style={{ marginTop: 11 }}>
            <div className="lab">
              {periodLabel}の架電数（人別・上位{cv.rows.length}名）
            </div>
            <div className="tw">
              <table data-testid="calls-by-person">
                <thead>
                  <tr>
                    <th>担当</th>
                    <th>チーム</th>
                    <th className="n">架電数</th>
                    <th className="n">発信回数</th>
                    <th className="n">つながった率</th>
                    <th className="n">5分超</th>
                    {cv.hasPrev ? <th className="n">前週比</th> : null}
                  </tr>
                </thead>
                <tbody>
                  {cv.rows.map((r) => {
                    const d2 = r.prev == null ? null : r.conn - r.prev;
                    return (
                      <tr key={r.id} data-owner={r.id}>
                        <td>{r.name}</td>
                        <td style={sub}>{r.team}</td>
                        <td className="n">
                          <b>{fmt(r.conn)}</b>
                        </td>
                        <td className="n" style={sub}>
                          {fmt(r.calls)}
                        </td>
                        <td className="n">{pct(r.calls ? (r.conn / r.calls) * 100 : null)}</td>
                        <td className="n">{fmt(r.lng)}</td>
                        {cv.hasPrev ? (
                          <td className="n" style={{ color: d2 !== null && d2 > 0 ? 'var(--ok)' : d2 !== null && d2 < 0 ? 'var(--alert)' : 'var(--faint)' }}>
                            {d2 !== null && d2 > 0 ? '+' : ''}
                            {fmt(d2)}
                          </td>
                        ) : null}
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
            <div className="hint" style={{ marginTop: 8 }}>
              担当者に紐づいた発信は {fmt(cv.calls)}件{hid ? '（チェックを外した人を除いた数）' : ''} ／ 全体 {fmt(cv.totalCalls)}件
              {hid ? (
                <>
                  （こちらは<b>絞り込み前</b>）
                </>
              ) : null}
              。紐づかない分は他部署の発信です（{cv.unmatchedTop.join(' ／ ')}）。
            </div>
          </div>
        ) : null}
      </div>
    </>
  );
}

// ---------------------------------------------------------------- 架電リストの残り

const KADEN_COLORS = { 未架電: 'var(--warn)', 未接触: 'var(--alert)', 接触済み: 'var(--accent)' } as const;

function KadenListSection({ data, ui, teamOf, hid, haveKettei }: { data: SalesKpiData; ui: UiState; teamOf: Record<string, string>; hid: boolean; haveKettei: boolean }) {
  const kv = kadenListView(data, ui.scope, teamOf);
  const k = data.kaden;
  const sc = kv.scope;
  let lead: ReactNode;
  let body: ReactNode = null;
  if (!sc) {
    lead = (
      <>
        <b style={warn}>担当者ごとの内訳がまだ集計されていません。</b>
        <span style={faint}>（日次同期が KPI営業_架電リスト_担当別 を作ると出ます）</span>
      </>
    );
  } else if (!sc.base) {
    lead = <>{sc.who}：架電リストに取引がありません。</>;
  } else {
    const g = (key: string): number => sc.c[key] ?? 0;
    const tr = sc.whole ? k.base_trend : null;
    lead = (
      <>
        リストのどこまで手がついているか。<b>{sc.who}</b>の数字です。
        {sc.whole ? (
          k.has_by_owner ? (
            <>
              <br />
              <span style={faint}>名簿でチームが決まっている人が持っている分だけを数えています。まだ誰にも配られていない分は下に別枠で出しています。</span>
            </>
          ) : null
        ) : (
          <>
            <br />
            <span style={faint}>チームや個人を選ぶと、その担当者が持っている取引だけで数え直します。</span>
          </>
        )}
      </>
    );
    const ug = (key: string): number => kv.unCls[key] ?? 0;
    body = (
      <>
        <div className="cards" id="cards3b">
          {kv.cards.map((o) => (
            <Card key={o.key} o={o} />
          ))}
        </div>
        <div id="listbar">
          <div className="c" style={{ marginTop: 11 }}>
            <div className="bar">
              {KADEN_CLASSES.map((key) => (
                <span key={key} style={{ width: `${String(kv.widths[key])}%`, background: KADEN_COLORS[key] }} />
              ))}
            </div>
            <div className="blegend">
              {KADEN_CLASSES.map((key) => (
                <span key={key}>
                  <i style={{ background: KADEN_COLORS[key] }} />
                  {key} {fmt(g(key))}件
                </span>
              ))}
            </div>
            <div className="hint" style={{ marginTop: 10 }}>
              決定者・決裁者の入力状況（<b>アポ前リスト全体</b> {fmt(k.total)}件{haveKettei ? '' : '・チームや個人では出せません'}
              {hid ? (
                <>
                  。<b style={warn}>チェックの絞り込みは効きません</b>）:{' '}
                </>
              ) : (
                '）: '
              )}
              {kv.fillParts.join(' ／ ')}{'　'}<b style={alert}>ほぼ入っていません。</b>
              {haveKettei ? (
                <>
                  <br />
                  担当者ごとの件数は上の<b>「決定者・決裁者」タブ</b>にあります。
                </>
              ) : null}
            </div>
          </div>
          {kv.showUnassigned ? (
            <div className="c" style={{ marginTop: 11 }} data-testid="unassigned">
              <div className="lab">まだ配られていないリスト</div>
              <div className="v">
                {fmt(kv.unCls.base)}
                <small>件</small>
              </div>
              <div className="hint">
                名簿でチームが決まっていない人が持っている分と、担当者が入っていない分。<b>上の「{sc.who}」には入っていません。</b>
              </div>
              <div className="sub2">
                まだかけていない {fmt(ug('未架電'))}件 ／ つながらず {fmt(ug('未接触'))}件 ／ 話せた {fmt(ug('接触済み'))}件
              </div>
              {kv.unassignedRows.length || kv.noOwn ? (
                <div className="tw" style={{ marginTop: 8 }}>
                  <table>
                    <thead>
                      <tr>
                        <th>誰が持っているか</th>
                        <th>所属</th>
                        <th className="n">件数</th>
                        <th className="n">まだかけていない</th>
                      </tr>
                    </thead>
                    <tbody>
                      {kv.unassignedRows.map((p) => (
                        <tr key={p.id}>
                          <td>{p.name}</td>
                          <td style={sub}>{p.hsTeam || '—'}</td>
                          <td className="n">
                            <b>{fmt(p.base)}</b>
                          </td>
                          <td className="n">{fmt(p['未架電'])}</td>
                        </tr>
                      ))}
                      {kv.noOwn ? (
                        <tr>
                          <td>
                            （担当者が入っていない）<span className="sm">担当者が居ないのでチェックでは外せません</span>
                          </td>
                          <td style={sub}>—</td>
                          <td className="n">
                            <b>{fmt(kv.noOwn)}</b>
                          </td>
                          <td className="n">—</td>
                        </tr>
                      ) : null}
                    </tbody>
                  </table>
                </div>
              ) : null}
              <div className="hint" style={{ marginTop: 10 }}>
                アポ前リスト全体では <b>{fmt(k.all.base)}件</b>
                {tr ? (
                  <>
                    （{tr.week} の記録 {fmt(tr.base)}件 より <b>{signed(tr.diff)}</b>）
                  </>
                ) : null}
                。<b>この母数そのものが動きます。</b>アポ前リストと BPO リストの間で毎月まとまった件数が行き来しています（リスト管理の通常の運用です）。そのため<b>率が動いても、そのまま「進んだ／戻った」とは限りません</b>。この画面では増減の理由までは判定できないので、大きく動いた月はリストを動かした人に確認してください。
              </div>
            </div>
          ) : null}
        </div>
      </>
    );
  }
  return (
    <>
      <h2>架電リストの残り</h2>
      <p className="lead" id="lead3b">
        {lead}
      </p>
      {body}
    </>
  );
}

// ---------------------------------------------------------------- 先週との比べ方

function SnapSection({ data, ui, actions, hid }: { data: SalesKpiData; ui: UiState; actions: UiActions; hid: boolean }) {
  const sv = snapView(data.snapshots, ui.snapMode);
  const cur = SNAP[ui.snapMode];
  return (
    <>
      <h2>先週との比べ方</h2>
      <p className="lead" id="lead4">
        {sv.few ? (
          <>
            週ごとの記録をこれから貯めていきます。今週が1週目なので、比較は来週から出せます。
            <br />
          </>
        ) : null}
        <b>商談の数え方を2つ持っています。下のボタンで切り替えてください。</b>
        <br />
        <b>{SNAP.week.chip}</b>＝{SNAP.week.note}
        <b>{SNAP.week.noteBold}</b>
        {SNAP.week.noteTail}
        <br />
        <b>{SNAP.month.chip}</b>＝{SNAP.month.note}
        <b style={warn}>{SNAP.month.noteBold}</b>
        {SNAP.month.noteTail}
        <br />
        <span style={faint}>
          この表は<b>全社</b>の記録です。上のチーム・個人の絞り込みには連動しません。
          {hid ? (
            <>
              <b style={warn}>担当者のチェックも効きません</b>（記録した時点の値をそのまま出しているため）。
            </>
          ) : null}
        </span>
      </p>
      <div className="chiprow" id="snapmode" style={{ marginBottom: 11 }}>
        {(['week', 'month'] as const).map((k) => (
          <button
            type="button"
            key={k}
            className={'chip' + (k === ui.snapMode ? ' on' : '')}
            onClick={() => {
              actions.setSnapMode(k);
            }}
          >
            {SNAP[k].chip}
          </button>
        ))}
      </div>
      <div id="snapbox">
        <div className="c">
          <div className="tw">
            <table data-testid="snapshots">
              <thead>
                <tr>
                  <th rowSpan={2}>週</th>
                  <th className="n grp" colSpan={3}>
                    {cur.head}
                  </th>
                  <th className="n" rowSpan={2}>
                    架電数<span className="sm">その週の合計</span>
                  </th>
                  <th className="n" rowSpan={2}>
                    取ったアポ<span className="sm">当月の累計</span>
                  </th>
                  <th className="n" rowSpan={2}>
                    止まっている<span className="sm">記録した時点</span>
                  </th>
                  <th className="n" rowSpan={2}>
                    架電リスト母数<span className="sm">記録した時点</span>
                  </th>
                </tr>
                <tr>
                  <th className="n">商談</th>
                  <th className="n">やった</th>
                  <th className="n">商談化率</th>
                </tr>
              </thead>
              <tbody>
                {sv.rows.map((s) => (
                  <tr key={s.week} data-week={s.week}>
                    <td>
                      {s.week}
                      <br />
                      <span style={{ color: 'var(--faint)', fontSize: '11.5px' }}>{md(s.weekStart)} の週</span>
                    </td>
                    {s.cells.kind === 'data' ? (
                      <>
                        <td className="n">
                          {s.cells.pool}
                          {s.cells.poolPartial ? <span className="sm">週の途中（集計中）</span> : null}
                        </td>
                        <td className="n">{s.cells.done}</td>
                        <td className="n">{s.cells.rate}</td>
                      </>
                    ) : (
                      <td className="n" colSpan={3}>
                        <span style={faint}>この数え方で記録する前の週です</span>
                      </td>
                    )}
                    <td className="n">
                      {s.zoom.text === null ? (
                        <span style={faint}>—</span>
                      ) : (
                        <>
                          {s.zoom.text}
                          {s.zoom.partialNote ? <span className="sm">{s.zoom.partialNote}</span> : null}
                        </>
                      )}
                    </td>
                    <td className="n">{s.apo}</td>
                    <td className="n">{s.stale}</td>
                    <td className="n">
                      {s.base ?? <span style={faint}>—</span>}
                      {s.baseDiff ? <span className="sm">{signed(s.baseDiff)}</span> : null}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="hint" style={{ marginTop: 8 }}>
            いま出しているのは<b>「{cur.head}」</b>です。{cur.note}
            {ui.snapMode === 'month' ? <b style={warn}>{cur.noteBold}</b> : <b>{cur.noteBold}</b>}
            {cur.noteTail}
            <br />
            {ui.snapMode === 'week' ? (
              <>
                その週の数字は<b>週が終わるまで確定しません</b>（残りの日ぶんが「これから」に入るため）。週が終わったあと、翌週の取り直しで実施・未実施を入れ直します。
              </>
            ) : (
              '「取ったアポ」も当月の累計なので、この列も月初にリセットされます。'
            )}
            <br />
            {sv.missing ? (
              <>
                「この数え方で記録する前の週です」と出ている行は、週ごとの数え方を足した 2026-09-07 より前に記録したものです。当月の累積なら見られます。
                <br />
              </>
            ) : null}
            この画面を開いたときではなく、データを取り直したときに1週ぶんが記録されます。週の途中で取り直すと、その週の行が上書きされます。架電数はZoomでつながった通話の数です。
            <br />
            <b>架電リスト母数</b>はアポ前リストの件数です。BPOリストとの間で件数が行き来するため週ごとに動きます。<b>母数が動いた週は、率の比較が成り立ちません。</b>
          </div>
        </div>
      </div>
    </>
  );
}

// ---------------------------------------------------------------- 決定者・決裁者タブ

function KetteiTab({ data, ui }: { data: SalesKpiData; ui: UiState }) {
  const ke = data.kettei;
  const kv = ketteiView(data, ui.scope);
  // 担当なしの行。const に取り出しておくと map の中でも null でないことが型に残る。
  const no = kv.no;
  const cell = (n: number | null): ReactNode => (
    <td className="n" style={{ color: toneColor(growTone(n)) }}>
      {growText(n)}
    </td>
  );
  return (
    <div id="tab-kettei" role="tabpanel">
      <h2>決定者・決裁者の入力状況</h2>
      <p className="lead" id="lead5">
        取引に決定者・決裁者がどれだけ入力されているかを、担当者ごとに数えたものです。<b>毎朝6:30 に取り直した数</b>で、いま出しているのは <b>{kv.asof}朝</b>の時点です。
        {ke.prev_date ? (
          <>「本日増加」は前の記録（{md(ke.prev_date)} 朝）からの増加です。</>
        ) : (
          <b style={warn}>前の記録がまだ無いので、「本日増加」はまだ出せません。</b>
        )}
        <br />
        <span style={faint}>
          現場が日中に入力したぶんは、翌朝の行に載ります。合計は<b>入力された項目の数</b>です。決定者名と決裁者名の両方が入っている取引は 2 と数えるので、<b>取引の件数ではありません</b>。
          <br />
          上のチーム・個人・担当者のチェックは、この表にも効きます。
        </span>
      </p>
      <div id="ketteibox">
        {!kv.rows.length && !kv.no ? (
          <div className="empty">選んでいる範囲に、決定者・決裁者を入力した担当者がいません。</div>
        ) : (
          <div className="c">
            <div className="tw">
              <table data-testid="kettei">
                <thead>
                  <tr>
                    <th>担当者</th>
                    {kv.cols.map((c) => (
                      <th className="n" key={c}>
                        {c}
                      </th>
                    ))}
                    <th className="n">
                      合計<span className="sm">項目の数</span>
                    </th>
                    <th className="n">
                      本日増加
                      {ke.prev_date ? <span className="sm">{md(ke.prev_date)} 朝からの増加</span> : <span className="sm">前の記録がありません</span>}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {kv.rows.map((r) => (
                    <tr key={r.owner} data-owner={r.owner}>
                      <td>{r.ownerName}</td>
                      {kv.cols.map((c) => (
                        <td className="n" key={c}>
                          {fmt(ketteiCell(r, c))}
                        </td>
                      ))}
                      <td className="n">
                        <b>{fmt(r['合計'])}</b>
                      </td>
                      {cell(r['増加'])}
                    </tr>
                  ))}
                  {no ? (
                    <tr data-owner="">
                      <td>
                        （担当者が入っていない）<span className="sm">担当者が居ないのでチェックでは外せません</span>
                      </td>
                      {kv.cols.map((c) => (
                        <td className="n" key={c}>
                          {fmt(ketteiCell(no, c))}
                        </td>
                      ))}
                      <td className="n">
                        <b>{fmt(no['合計'])}</b>
                      </td>
                      {cell(no['増加'])}
                    </tr>
                  ) : null}
                </tbody>
                <tfoot>
                  <tr>
                    <td>
                      合計
                      <span className="sm">
                        {kv.who}・{kv.rows.length}名{kv.no ? '＋担当なし' : ''}
                      </span>
                    </td>
                    {kv.cols.map((c) => (
                      <td className="n" key={c}>
                        {fmt(kv.sum[c])}
                      </td>
                    ))}
                    <td className="n">{fmt(kv.sum['合計'])}</td>
                    <td className="n">
                      {kv.grewKnown ? (
                        <>
                          <span style={{ color: toneColor(growTone(kv.grew)) }}>{growText(kv.grew)}</span>
                          {kv.miss ? <span className="sm">{kv.miss}行は前の記録なし</span> : null}
                        </>
                      ) : (
                        '—'
                      )}
                    </td>
                  </tr>
                </tfoot>
              </table>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- リストの在庫タブ

function StockHead({ r }: { r: StockRow }) {
  return (
    <td className="fc">
      {r.label}
      {r.note ? <span className="sm">{r.note}</span> : null}
    </td>
  );
}

function NamedLine({ named, n, show }: { named: number; n: number; show: boolean }) {
  if (!show) return null;
  return (
    <>
      <span className="sm">名前あり {fmt(named)}</span>
      <span className="sm">件数の {lsPct(named, n)}</span>
    </>
  );
}

function StockTab({ data }: { data: SalesKpiData }) {
  const so: StockOverview = stockOverview(data.list_stock);
  const per = so.named ? 2 : 1;
  const notes: ReactNode[] = [];
  if (!so.hasGroups) {
    notes.push(
      <span key="nogroups">
        <b style={warn}>区分がまだ決まっていません。</b>
        <SheetA k="listGroups" label="KPI営業_リスト区分" /> に「区分（アクティブ／保管）・内訳・種別（ownerId／名簿チーム／HubSpotチーム）・値」を書くと、翌朝から分けて出します。いまは全部「その他」に入っています。
      </span>,
    );
  }
  const trd = data.list_stock.trend;
  if (!trd) {
    notes.push(<span key="notrend">前の週の記録がまだ無いので、増減はまだ出せません。</span>);
  } else if (so.trendParts?.length) {
    notes.push(
      <span key="trend" data-testid="stock-trend">
        {trd.week} の記録（{md(trd.week_start)} の週）からの件数の増減:{' '}
        {so.trendParts.map((p, i) => {
          const sp = p.indexOf(' ');
          return (
            <span key={i}>
              {i ? '　' : ''}
              <b>{p.slice(0, sp)}</b>
              {p.slice(sp)}
            </span>
          );
        })}
        <br />
        リストはパイプラインの間・担当者の間で毎月まとまった件数が動きます。増減の理由まではこの画面では判定できません。
      </span>,
    );
  }
  const pair = (n: number, named: number, d: number, isSub: boolean | undefined, key: string): ReactNode => (
    <Fragment key={key}>
      <td className="n">
        {isSub ? (
          <>
            <b>{fmt(n)}</b>
            <span className="sm">全体の {lsPct(n, d)}</span>
          </>
        ) : (
          fmt(n)
        )}
      </td>
      {so.named ? (
        <td className="n">
          {fmt(named)}
          <span className="sm">件数の {lsPct(named, n)}</span>
        </td>
      ) : null}
    </Fragment>
  );
  const cols = [...so.bands, so.allBand];
  return (
    <div id="tab-stock" role="tabpanel">
      <h2>リストの在庫</h2>
      <p className="lead" id="lead6">
        新規営業のリストが<b>全体でどれだけあり、どれだけアクティブに回していて、どれだけ保管しているか</b>を数えたものです。<b>毎朝6:30 に取り直した数</b>（{data.generated_at} 時点）で、日中に動かしたぶんは翌朝に載ります。
        <br />
        <span style={faint}>
          <b>リクロジ</b> = パイプライン「リクロジ受注管理_アポ前」、<b>大分</b> = パイプライン「bpo_リクロジ」。どちらも<b>全ステージ</b>を数えています（架電禁止・ターゲット外なども入っています）。「bpo_リクロジ（管理）」（要精査・完全廃棄など）は入れていません。
          <br />
          誰をアクティブ・保管とするかは <SheetA k="listGroups" label="KPI営業_リスト区分" /> で決まります。どこにも当てはまらない担当者と、担当者が入っていない取引は「その他」に入れています。
          <br />
          {so.named ? (
            <>
              「名前あり」は、担当者名に<b>人の名前</b>が入っている取引です。空・「不明」・「担当」を含むもの（担当者・採用担当者 など）・役職や性別だけ（社長・女性 など）・記号だけの値は除いています。
              <br />
            </>
          ) : null}
          <b>上のチーム・個人・担当者のチェックは、この表には効きません</b>（担当者ごとではなく、区分ごとに数えているため）。
        </span>
      </p>
      <div id="stockbox">
        <div className="c">
          <div className="tw" style={{ maxHeight: 'none' }}>
            <table className="stk" data-testid="stock-overview">
              <thead>
                <tr>
                  <th className="fc" rowSpan={per}>
                    区分・内訳
                  </th>
                  {so.cols.map((c) => (
                    <th className={'n' + (so.named ? ' grp' : '')} colSpan={per} key={c}>
                      {c}
                    </th>
                  ))}
                </tr>
                {so.named ? (
                  <tr>
                    {so.cols.map((c) => (
                      <Fragment key={c}>
                        <th className="n">件数</th>
                        <th className="n">名前あり</th>
                      </Fragment>
                    ))}
                  </tr>
                ) : null}
              </thead>
              <tbody>
                {so.rows.map(({ row, ns, ms }, i) => (
                  <tr key={i} className={row.sub ? 'sub' : undefined} data-row={row.label}>
                    <StockHead r={row} />
                    {ns.map((n, j) => pair(n, ms[j] ?? 0, so.whole[j] ?? 0, row.sub, String(j)))}
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr>
                  <td className="fc">
                    全体<span className="sm">企業人数で絞っていない</span>
                  </td>
                  {so.whole.map((n, j) => (
                    <Fragment key={j}>
                      <td className="n">{fmt(n)}</td>
                      {so.named ? (
                        <td className="n">
                          {fmt(so.wholeNamed[j])}
                          <span className="sm">件数の {lsPct(so.wholeNamed[j] ?? 0, n)}</span>
                        </td>
                      ) : null}
                    </Fragment>
                  ))}
                </tr>
              </tfoot>
            </table>
          </div>
          <div className="hint" style={{ marginTop: 10 }}>
            {notes.map((n, i) => (
              <span key={i}>
                {i ? <br /> : null}
                {n}
              </span>
            ))}
          </div>
        </div>
      </div>

      <h2>企業人数の帯ごと</h2>
      <p className="lead" id="lead7">
        上の表を、企業人数（企業全体人数）の帯に分けたものです。リストごとに1つの表で、<b>帯を横に並べて</b>見比べられるようにしています。
        <br />
        <span style={faint}>
          升目のいちばん上の数が件数です。<b>アクティブの計・保管の計・その他</b>の行の「全体の ○%」は、<b>その列（帯）の全体</b>（いちばん下の行）に占める割合で、3つを足すと 100% になります。
          {so.named ? <>「名前あり」はそのうち担当者名に人の名前が入っている件数で、「件数の ○%」の母数は<b>同じ升目の件数</b>です。</> : null}
          「計」の列は企業人数で絞らない数です。
        </span>
      </p>
      <div id="stockbybands">
        {so.lists.map((l, i) => (
          <div className="c" style={{ marginTop: i ? 12 : 0 }} key={l.name} data-testid={'stock-bands-' + l.name}>
            <div className="ph">
              <b>{l.name}</b>
            </div>
            <div className="tw" style={{ maxHeight: 'none' }}>
              <table className="stk">
                <thead>
                  <tr>
                    <th className="fc">区分・内訳</th>
                    {cols.map((b) => (
                      <th className="n" key={b}>
                        {b === so.allBand ? '計' : b}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {(so.rowsBy[i] ?? []).map((r, ri) => (
                    <tr key={ri} className={r.sub ? 'sub' : undefined} data-row={r.label}>
                      <StockHead r={r} />
                      {cols.map((b) => {
                        const n = lsGet(r.n, b);
                        const named = lsGet(r.named, b);
                        return (
                          <td className="n" key={b}>
                            {r.sub ? (
                              <>
                                <b>{fmt(n)}</b>
                                <span className="sm">全体の {lsPct(n, lsGet(l.total, b))}</span>
                              </>
                            ) : (
                              fmt(n)
                            )}
                            <NamedLine named={named} n={n} show={so.named} />
                          </td>
                        );
                      })}
                    </tr>
                  ))}
                </tbody>
                <tfoot>
                  <tr>
                    <td className="fc">全体</td>
                    {cols.map((b) => (
                      <td className="n" key={b}>
                        {fmt(lsGet(l.total, b))}
                        <NamedLine named={lsGet(l.total_named, b)} n={lsGet(l.total, b)} show={so.named} />
                      </td>
                    ))}
                  </tr>
                </tfoot>
              </table>
            </div>
            {l.band_gap ? (
              <div className="hint" style={{ marginTop: 10 }}>
                企業人数がどの帯にも入らない取引が {fmt(l.band_gap)}件 あります（帯の列を足しても「計」にこのぶん届きません）。
              </div>
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- 画面全体

export function SalesKpiView({ data, ui, actions }: SalesKpiViewProps) {
  const teamOf = teamOfMap(data.people);
  const hid = hiddenCount(data.people, ui.scope.hidden) > 0;
  const haveKettei = hasKettei(data);
  const tabs = tabsOf(data);
  const mv = monthView(data, ui.scope, teamOf);
  const av = actionView(data, ui.scope);
  const ex = excludedParts(data.excluded);
  const hiddenN = hiddenCount(data.people, ui.scope.hidden);
  const people = visiblePeople(data.people, ui.scope);
  return (
    <div className="wrap">
      <header>
        <div>
          <h1>営業KPI</h1>
          <p className="range" id="range">
            {rangeText(data)}
          </p>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}>
          <a className="backlink" href="/">
            ← ダッシュボードへ戻る
          </a>
          <button type="button" className="hbtn" id="tt" onClick={actions.toggleTheme}>
            ◐ 表示切替
          </button>
        </div>
      </header>

      {tabs.length > 1 ? (
        <div className="tabs" id="tabs" role="tablist">
          {tabs.map((t) => (
            <button
              type="button"
              key={t.key}
              className={'tab' + (t.key === ui.tab ? ' on' : '')}
              role="tab"
              aria-selected={t.key === ui.tab}
              aria-controls={'tab-' + t.key}
              onClick={() => {
                actions.setTab(t.key);
              }}
            >
              {t.label}
            </button>
          ))}
        </div>
      ) : null}

      <div className="filters">
        <div className="chiprow" id="teams">
          {[ALL_TEAMS, ...data.teams].map((t) => (
            <button
              type="button"
              key={t}
              className={'chip' + (t === ui.scope.team ? ' on' : '')}
              onClick={() => {
                actions.setTeam(t);
              }}
            >
              {t}
            </button>
          ))}
        </div>
        <select
          id="person"
          value={ui.scope.person}
          onChange={(e) => {
            actions.setPerson(e.target.value);
          }}
        >
          <option value="">個人で見る…</option>
          {people.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name + (ui.scope.team === ALL_TEAMS ? '（' + teamLabel(p) + '）' : '')}
            </option>
          ))}
        </select>
        <button type="button" className="hbtn" id="pickbtn" onClick={actions.togglePick}>
          {ui.pickOpen ? '担当者を閉じる' : '担当者を選ぶ'}
        </button>
        <span id="pickcount" style={{ fontSize: '12.5px' }}>
          {hiddenN ? <b style={warn}>{hiddenN}名を外しています</b> : <span style={faint}>{data.people.length}名すべて入っています</span>}
        </span>
      </div>
      {ui.pickOpen ? <PickPanel people={data.people} scope={ui.scope} actions={actions} /> : null}
      <p className="scope" id="scope">
        {scopeText(data, ui.scope, mv.a)}
      </p>

      <div id="tab-kpi" role="tabpanel" hidden={ui.tab !== 'kpi'}>
        <p className="fixlinks" id="fixlinks">
          チーム分け・在籍を直す：<SheetA k="roster" label="名簿を開く" />
          <span className="sh">（{SHEETS.roster.sheet}）</span>
          商談の集計から人を外す：<SheetA k="exclude" label="集計除外を開く" />
          <span className="sh">（{SHEETS.exclude.sheet}）</span>
        </p>

        <h2>今月の成績</h2>
        <p className="lead">
          商談の予定日が過ぎたものだけで数えています（今日の商談はまだ終わっていないので入れません）。
          <br />
          <span id="bporule">
            <b>内 BPO</b> は「BPOアポ取得日」がその期間にあるものです。このプロパティは<b>過去のBPOアポの日付が残り続ける</b>ため、値の有無では判定できません（今月の商談で言うと、値があるのは200件ですが、そのうち当月・前月に取ったものは129件です）。
          </span>
        </p>
        <p className="scope" id="scope2" style={{ margin: '-8px 0 12px' }}>
          {ex ? (
            <>
              商談のもとデータからは <b>{ex.parts.join(' ／ ')}</b> を除いています
              <span style={faint}>
                （誰を除くかは <SheetA k="exclude" label="KPI営業_集計除外" /> で変えられます。架電と架電リストは除いていません）。
              </span>
            </>
          ) : null}
        </p>
        <div className="cards" id="cards1">
          {mv.cards.map((o) => (
            <Card key={o.key} o={o} />
          ))}
        </div>

        <h2>いま手を打てること</h2>
        <p className="lead">数字を押すと、中身の一覧が開きます。そのままHubSpotを開けます。</p>
        <div className="cards" id="cards2">
          {av.cards.map((o) => (
            <Card
              key={o.key}
              o={o}
              open={ui.openKey === o.key}
              onClick={() => {
                actions.toggleOpen(o.key);
              }}
            />
          ))}
        </div>
        <Panel data={data} ui={ui} actions={actions} av={av} />

        <CallsSection data={data} ui={ui} actions={actions} teamOf={teamOf} ab={mv.ab} hid={hid} />

        <KadenListSection data={data} ui={ui} teamOf={teamOf} hid={hid} haveKettei={haveKettei} />

        <SnapSection data={data} ui={ui} actions={actions} hid={hid} />

        <div className="foot" id="foot">
          カードの下の小さい「1人あたり」は、選んでいるチームの合計を<b>そのチームの人数</b>（個人プルダウンに出る人数）で割ったものです。個人を選んだときは、その人が所属するチームの平均を出しています。率のカード（商談化率・アンケート回収率・つながった率）には付けていません。
          <br />
          ⑧ 決定者以上の割合は、判定できる材料が足りないためまだ出せません（事前アンケートの回収が必要です）。
          <br />
          「ステージが止まっている」は商談予定日を過ぎてもステージが「アポ日確定」のままのもの（直近{data.stale_days}日ぶん）。ステージが止まっているだけで、商談をしたかどうかは分かりません。
          <br />
          架電数は Zoom のログで「つながった通話」を数えたものです。相手が出る前に切った発信・失敗した発信は「発信した回数」にだけ入ります。
          <br />
          誰がかけたかは、Zoomのメールアドレスと HubSpot 担当者のメールアドレスで突き合わせています{matchedText(data)}。
          <br />
          データ取得: {data.generated_at} ／ HubSpot・Zoom（読み取りのみ）。自動更新はまだ行っていません。
        </div>
      </div>

      {haveKettei && ui.tab === 'kettei' ? <KetteiTab data={data} ui={ui} /> : null}
      {tabs.some((t) => t.key === 'stock') && ui.tab === 'stock' ? <StockTab data={data} /> : null}
    </div>
  );
}
