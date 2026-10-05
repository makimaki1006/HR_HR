// 営業KPI: HubSpot 直読みの状態表示。サーバ (src/handlers/sales_kpi/hubspot_direct.rs `overlay_meta`) が
// `meta` に入れる文字列だけで決める。サーバの挙動は変えない。直読みが無効 (シート) のときは
// これらのキーが無いので null を返し、画面は今までと同じ。
// 旧画面 templates/tabs/sales_kpi.html の `directStatus` と同じ文言・同じ条件 (parity.direct.test.tsx で突き合わせる)。
// 時刻は `yyyy-MM-dd HH:mm` (JST) の文字列のまま使う。Date を通さないので実行環境の TZ に左右されない。

export type DirectKind = 'ok' | 'loading' | 'stale';

export interface DirectStatus {
  kind: DirectKind;
  /** 太字の 1 行目 */
  head: string;
  /** loading のときの補足 (他は null) */
  sub: string | null;
  /** どのブロックが即時でどれが朝のシート由来か */
  note: string;
}

/** loading のあいだ、この間隔で読み直す。 */
export const DIRECT_RELOAD_MS = 15_000;
/** 読み直しの上限回数 (15 秒 × 12 = 3 分)。超えたら止めて、手動の再読み込みを案内する。 */
export const DIRECT_RELOAD_MAX = 12;

const NOTE =
  'HubSpot の値(即時): 商談・アポ・Cヨミ・決定者の当日分・メンバー・架電リスト。' +
  '朝のシートの値: 担当別・リスト在庫・架電日次・週次・決定者の過去日。';

type MetaLike = Record<string, unknown> | null | undefined;

function kindOf(meta: MetaLike): DirectKind | null {
  if (meta === null || typeof meta !== 'object') return null;
  const v = meta['HubSpot取得状態'];
  return v === 'ok' || v === 'loading' || v === 'stale' ? v : null;
}

/** `yyyy-MM-dd HH:mm` だけ受け付ける。それ以外 (空・形式違い・範囲外) は null。 */
function parseTime(v: unknown): { full: string; hhmm: string } | null {
  if (typeof v !== 'string') return null;
  const m = /^\d{4}-\d{2}-\d{2} (\d{2}):(\d{2})$/.exec(v);
  if (!m) return null;
  const h = Number(m[1]);
  const mi = Number(m[2]);
  if (h > 23 || mi > 59) return null;
  return { full: v, hhmm: (m[1] ?? '') + ':' + (m[2] ?? '') };
}

export function directStatus(meta: MetaLike, exhausted = false): DirectStatus | null {
  const kind = kindOf(meta);
  if (kind === null || meta === null || meta === undefined) return null;
  const fetched = parseTime(meta['HubSpot取得時刻']);
  if (kind === 'loading') {
    return {
      kind,
      head: 'HubSpot から取得中です。数十秒かかります',
      sub:
        '取得が終わるまで、HubSpot の値は 0 件ではなく未取得です。' +
        (exhausted
          ? '自動の再読み込みを止めました。しばらくしてからページを再読み込みしてください。'
          : '15 秒ごとに自動で再読み込みします。'),
      note: NOTE,
    };
  }
  if (kind === 'stale') {
    const k = meta['HubSpot最終失敗種別'];
    const kindText = typeof k === 'string' && k !== '' ? k : '種別不明';
    const at = parseTime(meta['HubSpot最終失敗時刻']);
    return {
      kind,
      head:
        '最新の取得に失敗しました(' +
        kindText +
        '、' +
        (at ? at.full : '時刻不明') +
        ')。' +
        (fetched ? fetched.full + ' 時点の値を表示しています' : '前回取得した値を表示しています'),
      sub: null,
      note: NOTE,
    };
  }
  return {
    kind,
    head: 'HubSpot から直接取得: ' + (fetched ? fetched.hhmm + ' 時点' : '時刻不明') + '(5 分ごとに更新)',
    sub: null,
    note: NOTE,
  };
}

/** 次の読み直しまでの待ち (ms)。読み直さないなら null。attempts = これまでの読み直し回数。 */
export function nextReloadDelay(meta: MetaLike, attempts: number): number | null {
  if (kindOf(meta) !== 'loading') return null;
  return attempts < DIRECT_RELOAD_MAX ? DIRECT_RELOAD_MS : null;
}
