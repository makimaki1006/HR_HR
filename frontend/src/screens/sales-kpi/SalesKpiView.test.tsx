// 画面 (SalesKpiView) を fixture で描いて、表示される値を具体値で確かめる。
// happy-dom が main に無いので renderToStaticMarkup で描き、ui 状態は props で切り替える
// (切替操作の結果 = 同じ props で描いたときの出力)。
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { loadFixture } from './__fixtures__/load';
import { ALL_TEAMS, DEFAULT_CALL_PERIOD, type Scope } from './calc';
import { SalesKpiView, type UiActions, type UiState } from './SalesKpiView';

const D = loadFixture();

const noop: UiActions = {
  setTeam: vi.fn(),
  setPerson: vi.fn(),
  setHidden: vi.fn(),
  resetHidden: vi.fn(),
  toggleOpen: vi.fn(),
  closePanel: vi.fn(),
  setDayKey: vi.fn(),
  setWeekOpen: vi.fn(),
  setCallPeriod: vi.fn(),
  setSnapMode: vi.fn(),
  setTab: vi.fn(),
  togglePick: vi.fn(),
  toggleTheme: vi.fn(),
};

const base: UiState = {
  scope: { team: ALL_TEAMS, person: null, hidden: new Set() },
  openKey: null,
  dayKey: null,
  weekOpen: false,
  callPeriod: DEFAULT_CALL_PERIOD,
  snapMode: 'week',
  tab: 'kpi',
  pickOpen: false,
};

function render(ui: Partial<UiState> = {}, scope: Partial<Scope> = {}): string {
  return renderToStaticMarkup(
    <SalesKpiView data={D} ui={{ ...base, ...ui, scope: { ...base.scope, ...ui.scope, ...scope } }} actions={noop} />,
  );
}

/** `<... data-card="key"><div class="lab">…</div><div class="v">VALUE<small>…` の VALUE (unit を除く)。 */
function cardValue(html: string, key: string): string {
  const m = new RegExp(`data-card="${key}"><div class="lab">[^<]*</div><div class="v">([^<]*)`).exec(html);
  if (!m?.[1]) throw new Error(`card ${key} not found`);
  return m[1];
}
function cardBlock(html: string, key: string): string {
  const i = html.indexOf(`data-card="${key}"`);
  if (i < 0) throw new Error(`card ${key} not found`);
  const j = html.indexOf('data-card=', i + 10);
  return html.slice(i, j < 0 ? undefined : j);
}
/** 表の tbody の各行のセル文字列 (タグを落とす)。 */
function tableRows(html: string, testId: string, section: 'tbody' | 'tfoot' = 'tbody'): string[][] {
  const start = html.indexOf(`data-testid="${testId}"`);
  if (start < 0) throw new Error(`table ${testId} not found`);
  const end = html.indexOf('</table>', start);
  const table = html.slice(start, end);
  const secStart = table.indexOf(`<${section}>`);
  const secEnd = table.indexOf(`</${section}>`);
  const body = table.slice(secStart, secEnd);
  return [...body.matchAll(/<tr[^>]*>([\s\S]*?)<\/tr>/g)].map((tr) =>
    [...(tr[1] ?? '').matchAll(/<t[dh][^>]*>([\s\S]*?)<\/t[dh]>/g)].map((td) => (td[1] ?? '').replace(/<[^>]+>/g, '')),
  );
}

describe('ヘッダー・絞り込み', () => {
  const html = render();
  it('見出し・期間・戻るリンク・表示切替', () => {
    expect(html).toContain('<h1>営業KPI</h1>');
    expect(html).toContain('<p class="range" id="range">2026年9月　／　今週 8/31（月）〜9/6（日）　※2026-09-05 19:08 時点</p>');
    expect(html).toContain('<a class="backlink" href="/">← ダッシュボードへ戻る</a>');
    expect(html).toContain('id="tt"');
  });
  it('タブ 3 つ、営業KPI が選択、他 2 つの中身は出さない', () => {
    expect(html).toContain('<button type="button" class="tab on" role="tab" aria-selected="true" aria-controls="tab-kpi">営業KPI</button>');
    expect(html).toContain('aria-controls="tab-kettei">決定者・決裁者</button>');
    expect(html).toContain('aria-controls="tab-stock">リストの在庫</button>');
    expect(html).not.toContain('id="tab-kettei"');
    expect(html).not.toContain('id="tab-stock"');
  });
  it('チーム chip は「すべて」+ teams の順、すべてが on', () => {
    const chips = [...html.matchAll(/<button type="button" class="chip( on)?">([^<]*)<\/button>/g)].map((m) => [m[2], !!m[1]]);
    expect(chips.slice(0, 7)).toEqual([
      ['すべて', true], ['チーム未設定', false], ['伊壺チーム', false], ['平田チーム', false], ['櫻井チーム', false], ['野中チーム', false], ['野口チーム', false],
    ]);
  });
  it('個人プルダウン: 109 名 + 「個人で見る…」、すべてのときはチーム名付き', () => {
    const opts = [...html.matchAll(/<option value="([^"]*)"[^>]*>([^<]*)<\/option>/g)];
    expect(opts).toHaveLength(110);
    expect(opts[0]?.[2]).toBe('個人で見る…');
    expect(opts[1]?.[2]).toBe('担当074（チーム未設定・BPO_リクロジ）');
  });
  it('チェック無し: 「109名すべて入っています」、scope 文', () => {
    expect(html).toContain('109名すべて入っています');
    expect(html).toContain('<p class="scope" id="scope">全チームの合計を表示しています。チーム名か、右のプルダウンで絞り込めます。</p>');
    expect(html).toContain('商談のもとデータからは <b>コンサル営業 7件</b> を除いています');
  });
});

describe('今月の成績 (すべて)', () => {
  const html = render();
  it('7 枚のカードの値・順', () => {
    expect(cardValue(html, 'apo')).toBe('245');
    expect(cardValue(html, 'pool')).toBe('537');
    expect(cardValue(html, 'den')).toBe('225');
    expect(cardValue(html, 'done')).toBe('163');
    expect(cardValue(html, 'rate')).toBe('72.4%');
    expect(cardValue(html, 'anq')).toBe('2.7%');
    expect(cardValue(html, 'cyomi')).toBe('123');
    const order = [...html.matchAll(/data-card="([a-z]+)"/g)].map((m) => m[1]).slice(0, 7);
    expect(order).toEqual(['apo', 'pool', 'den', 'done', 'rate', 'anq', 'cyomi']);
  });
  it('内 BPO・1人あたり・hint', () => {
    const apo = cardBlock(html, 'apo');
    expect(apo).toContain('<div class="hint">今月アポ日が確定した数</div>');
    expect(apo).toContain('内 BPO 51件（21%）');
    expect(apo).toContain('<div class="avg">1人あたり 2.2件<span style="opacity:0.75">（109名）</span></div>');
    expect(cardBlock(html, 'rate')).toContain('<div class="hint">163 ÷ 225 件</div>');
    expect(cardBlock(html, 'rate')).not.toContain('class="avg"');
    expect(cardBlock(html, 'anq')).toContain('6 ÷ 225 件（④ 日が過ぎた分と同じ母数）');
  });
});

describe('絞り込みを変えたとき', () => {
  it('伊壺チーム: カードの値と scope 文、プルダウン 7 名 (チーム名なし)', () => {
    const html = render({}, { team: '伊壺チーム' });
    expect(cardValue(html, 'apo')).toBe('31');
    expect(cardValue(html, 'pool')).toBe('66');
    expect(cardValue(html, 'rate')).toBe('63.3%');
    expect(html).toContain('伊壺チーム の数字だけを表示しています。');
    expect(html).toContain('class="chip on">伊壺チーム</button>');
    const opts = [...html.matchAll(/<option value="([^"]*)"[^>]*>([^<]*)<\/option>/g)];
    expect(opts).toHaveLength(8);
    expect(opts[1]?.[2]).toBe('担当001');
    expect(cardBlock(html, 'apo')).toContain('1人あたり 4.4件<span style="opacity:0.75">（7名）</span>');
  });
  it('個人 (担当001): 自分の値と「伊壺チーム の平均」', () => {
    const html = render({}, { person: '613211320' });
    expect(cardValue(html, 'apo')).toBe('10');
    expect(cardValue(html, 'rate')).toBe('60.0%');
    expect(html).toContain('担当001 の数字だけを表示しています。');
    expect(cardBlock(html, 'apo')).toContain('伊壺チーム の平均 4.4件');
    expect(html).toContain('<select id="person"><option value="">個人で見る…</option>');
    expect(html).toContain('<option value="613211320" selected="">');
  });
  it('チェックで 2 名外す: 件数が減り、注記が出る', () => {
    const html = render({}, { hidden: new Set(['613211320', '96032023']) });
    expect(html).toContain('2名を外しています');
    expect(html).toContain('　2名をチェックで外しています。');
    expect(cardValue(html, 'apo')).not.toBe('245');
    expect(html).toContain('チェックの絞り込みは効きません');
    expect(html).toContain('担当者のチェックも効きません');
  });
  it('担当者を選ぶパネル: チーム順 (営業チーム先、チーム未設定は最後)、人数', () => {
    const html = render({ pickOpen: true });
    expect(html).toContain('担当者を閉じる');
    expect(html).toContain('<b>数字に入れる担当者</b>');
    const groups = [...html.matchAll(/<div class="gh"><input type="checkbox"[^>]*\/><b>([^<]*)<\/b><span class="n">([^<]*)<\/span>/g)].map((m) => [m[1], m[2]]);
    // 営業チームは localeCompare('ja') 順 (旧画面と同じ)、チーム未設定は最後
    expect(groups).toEqual([
      ['伊壺チーム', '7 / 7名'], ['平田チーム', '9 / 9名'], ['野口チーム', '4 / 4名'], ['野中チーム', '5 / 5名'], ['櫻井チーム', '8 / 8名'], ['チーム未設定', '76 / 76名'],
    ]);
  });
});

describe('いま手を打てること', () => {
  it('5 枚のカードと色分け、閉じているとき panel は hide', () => {
    const html = render();
    expect(cardValue(html, 'stale')).toBe('9');
    expect(cardValue(html, 'anq')).toBe('2.7%'); // 今月の成績の ⑤ (同じ key は 1 つ目)
    expect(html).toContain('class="c alert" data-card="stale"');
    expect(html).toContain('class="c warn" data-card="anq"');
    expect(html).toContain('class="c warn" data-card="cyomi"');
    expect(html).toContain('<button type="button" class="c alert" data-card="stale"><div class="lab">⑦ ステージが止まっている</div><div class="v">9<small>件</small></div><div class="hint">商談日が過ぎたのに動いていない</div><div class="open">一覧を見る ▾</div></button>');
    expect(html).toContain('<div class="lab">③ 今週の商談</div><div class="v">260<small>件</small></div><div class="hint">うち 206件 は日が過ぎました</div>');
    expect(html).toContain('<div class="lab">③ 来週の商談</div><div class="v">213<small>件</small></div><div class="hint">9/7〜9/13 の予定</div>');
    expect(html).toContain('<div class="panel hide" id="panel"></div>');
  });
  it('⑦ を開く: 見出し (9件)、日付と「N日前」、stale 色', () => {
    const html = render({ openKey: 'stale' });
    expect(html).toContain('<b>ステージが止まっている取引（9件）</b>');
    expect(html).toContain('<div class="open">閉じる ▲</div>');
    // DealRow.url (HubSpot の取引ページ。#45 でサーバが付ける) がそのまま href になる
    expect(html).toContain(
      '<a class="item stale" href="https://app.hubspot.com/contacts/23708633/record/0-3/15873734455/" target="_blank" rel="noopener noreferrer"',
    );
    expect(html).toContain('<div class="d"><b>7/10（金）</b><span>57日前</span></div><div class="nm">（取引名なし）</div><div class="who">担当006</div><div class="go">HubSpotを開く ›</div>');
  });
  it('③ 今週を開く: 日別ストリップ (7 日、0 件の日も)、今日・過去の印', () => {
    const html = render({ openKey: 'week' });
    expect(html).toContain('<b>今週の商談（260件）</b>');
    const days = [...html.matchAll(/<button type="button" class="(wkday[^"]*)"[^>]*data-date="([^"]+)"><div class="wd">([^<]*)<\/div><div class="dt">([^<]*)<\/div><div class="n">(\d+)<span class="u">件<\/span>/g)].map((m) => [m[2], m[5], m[1]]);
    expect(days).toEqual([
      ['2026-08-31', '30', 'wkday past'],
      ['2026-09-01', '42', 'wkday past'],
      ['2026-09-02', '71', 'wkday past'],
      ['2026-09-03', '63', 'wkday past'],
      ['2026-09-04', '52', 'wkday'],
      ['2026-09-05', '0', 'wkday zero today'],
      ['2026-09-06', '2', 'wkday'],
    ]);
    expect(html).toContain('全部の日をまとめて見る ▾');
  });
  it('日を押したとき: その日の一覧 (done で薄く)', () => {
    const html = render({ openKey: 'week', dayKey: '2026-09-02' });
    expect(html).toContain('<div class="day">9/2（水）　71件</div>');
    expect((html.match(/class="item done"/g) ?? []).length).toBe(71);
    expect(html).toContain('この日を閉じる');
  });
  it('個人で ⑦ が 0 件のとき「該当はありません。」', () => {
    const html = render({ openKey: 'stale' }, { person: '613211320' });
    expect(html).toContain('<b>ステージが止まっている取引（0件）</b>');
    expect(html).toContain('<div class="empty"><b>該当はありません。</b></div>');
  });
});

describe('架電', () => {
  it('今週 (既定): カード・前週比・chip の on', () => {
    const html = render();
    expect(cardValue(html, 'conn')).toBe('29,866');
    expect(cardValue(html, 'calls')).toBe('34,295');
    expect(cardValue(html, 'ratio')).toBe('87.1%');
    expect(cardValue(html, 'long')).toBe('791');
    expect(cardValue(html, 'perday')).toBe('5,973');
    expect(cardBlock(html, 'conn')).toContain('<div class="wow" style="color:var(--ok)">▲4,982件 <span style="color:var(--faint)">先週 24,884件</span></div>');
    expect(cardBlock(html, 'conn')).toContain('<div class="hint">8/31〜9/4</div>');
    expect(cardBlock(html, 'conn')).toContain('Zoomでつながった通話の数（9/4 は集計中）');
    expect(cardBlock(html, 'perday')).not.toContain('class="avg"');
    const chips = /id="callperiod"[^>]*>([\s\S]*?)<\/div>/.exec(html)?.[1] ?? '';
    expect([...chips.matchAll(/class="chip( on)?">([^<]*)</g)].map((m) => [m[2], !!m[1]])).toEqual([
      ['今週', true], ['先週', false], ['今日', false], ['昨日', false], ['今月', false],
    ]);
    expect(html).toContain('9/4 は集計中の数です。');
    expect(html).toContain('（今週の発信 54,098件のうち 34,295件が突合できました。');
  });
  it('人別の表: 40 行、先頭は 担当407 949 件、前週比 +251', () => {
    const html = render();
    const rows = tableRows(html, 'calls-by-person');
    expect(rows).toHaveLength(40);
    expect(rows[0]).toEqual(['担当407', '櫻井チーム', '949', '1,126', '84.3%', '30', '+251']);
    expect(html).toContain('今週の架電数（人別・上位40名）');
    expect(html).toContain('担当者に紐づいた発信は 34,295件 ／ 全体 54,098件。紐づかない分は他部署の発信です（(不明) 1,358件 ／ Capital 20件 ／ FAC Capital 8,947件）。');
  });
  it('日別グラフ: 9 本、最大 12,291', () => {
    const html = render();
    expect(html).toContain('data-testid="daily-chart" data-bars="9"');
    expect(html).toContain('data-bar-date="2026-08-25" data-calls="12291" data-connected="10528"');
    expect(html).toContain('>12,291</text>');
  });
  it('先週に切り替え: 前週比の列が無い', () => {
    const html = render({ callPeriod: 'prev_week' });
    expect(cardValue(html, 'conn')).toBe('24,885');
    expect(tableRows(html, 'calls-by-person')[0]).toHaveLength(6);
    expect(html).toContain('先週の架電数（人別・上位40名）');
  });
});

describe('架電リストの残り', () => {
  it('すべて = 営業5チームの合計', () => {
    const html = render();
    expect(html).toContain('<b>営業5チームの合計</b>の数字です。');
    expect(cardValue(html, 'mikaden')).toBe('11,942');
    expect(cardValue(html, 'misesshoku')).toBe('15,327');
    expect(cardValue(html, 'sesshoku')).toBe('16,344');
    expect(cardValue(html, 'touched')).toBe('72.6%');
    expect(html).toContain('決定者の役職 0.2% ／ 決定者名 0.2% ／ 決裁者の役職 0.1% ／ 決裁者名 0.1%');
    expect(html).toContain('data-testid="unassigned"');
    expect(html).toContain('<div class="v">86,168<small>件</small></div>');
    // 差 0 は旧 JS と同じく「−0」(sign は n>0 のときだけ ＋)
    expect(html).toContain('アポ前リスト全体では <b>129,869件</b>（2026-W35 の記録 129,869件 より <b>−0</b>）');
  });
  it('伊壺チーム: 未配布の枠は出さない', () => {
    const html = render({}, { team: '伊壺チーム' });
    expect(html).toContain('<b>伊壺チーム が持っている分</b>の数字です。');
    expect(cardValue(html, 'touched')).toBe('50.6%');
    expect(html).not.toContain('data-testid="unassigned"');
  });
});

describe('先週との比べ方', () => {
  it('その週の商談 (既定)', () => {
    const html = render();
    const rows = tableRows(html, 'snapshots');
    expect(rows).toEqual([
      ['2026-W358/24 の週', '319', '224', '70.2%', '40,293', '245', '5', '129,869'],
      ['2026-W368/31 の週', '260週の途中（集計中）', '188', '74.9%', '46,1375日目まで（集計中）', '245', '9', '129,869'],
    ]);
    expect(html).toContain('<th class="n grp" colSpan="3">その週に予定された商談</th>');
  });
  it('当月の累積', () => {
    const html = render({ snapMode: 'month' });
    const rows = tableRows(html, 'snapshots');
    expect(rows[0]?.slice(1, 4)).toEqual(['1,063', '755', '72.9%']);
    expect(rows[1]?.slice(1, 4)).toEqual(['537', '163', '72.4%']);
    expect(html).toContain('当月に予定された商談（月初からの累積）');
  });
});

describe('決定者・決裁者タブ', () => {
  it('行・合計・増加', () => {
    const html = render({ tab: 'kettei' });
    expect(html).toContain('id="tab-kettei"');
    expect(html).not.toContain('id="tab-stock"');
    expect(html).toContain('いま出しているのは <b>9/7（月）朝</b>の時点です。「本日増加」は前の記録（9/5 朝）からの増加です。');
    const rows = tableRows(html, 'kettei');
    expect(rows[0]).toEqual(['担当001', '70', '69', '66', '84', '289', '+31']);
    expect(rows[3]).toEqual(['担当024', '22', '20', '18', '19', '79', '±0']);
    expect(rows[4]).toEqual(['担当029', '12', '11', '9', '10', '42', '-3']);
    expect(rows[5]).toEqual(['担当238', '5', '4', '3', '3', '15', '—']);
    expect(rows[6]).toEqual(['（担当者が入っていない）担当者が居ないのでチェックでは外せません', '8', '8', '6', '7', '29', '+3']);
    expect(tableRows(html, 'kettei', 'tfoot')).toEqual([['合計全社・6名＋担当なし', '202', '202', '177', '212', '793', '+1251行は前の記録なし']]);
    expect(html).toContain('style="color:var(--ok)">+31</td>');
    expect(html).toContain('style="color:var(--alert)">-3</td>');
  });
  it('伊壺チームに絞ると 1 名', () => {
    const html = render({ tab: 'kettei' }, { team: '伊壺チーム' });
    expect(tableRows(html, 'kettei')).toHaveLength(1);
    expect(tableRows(html, 'kettei', 'tfoot')[0]?.[0]).toBe('合計伊壺チーム・1名');
  });
});

describe('リストの在庫タブ', () => {
  it('概要表: 行と件数・割合、全体行', () => {
    const html = render({ tab: 'stock' });
    expect(html).toContain('id="tab-stock"');
    const rows = tableRows(html, 'stock-overview');
    expect(rows[0]).toEqual(['FSメンバーアクティブ', '48,868', '39,801件数の 81.4%', '3,674', '3,559件数の 96.9%', '52,542', '43,360件数の 82.5%']);
    expect(rows[2]?.[0]).toBe('アクティブの計');
    expect(rows[2]?.[1]).toBe('48,889全体の 29.8%');
    expect(rows[7]?.[0]).toBe('その他区分シートに書かれていない人・担当者なし');
    expect(rows[7]?.[1]).toBe('30,784全体の 18.8%');
    expect(tableRows(html, 'stock-overview', 'tfoot')[0]).toEqual([
      '全体企業人数で絞っていない', '164,159', '92,442件数の 56.3%', '96,105', '34,883件数の 36.3%', '260,264', '127,325件数の 48.9%',
    ]);
    expect(html).toContain('前の週の記録がまだ無いので、増減はまだ出せません。');
  });
  it('帯ごとの表: 列 = 帯 + 計、リクロジの保管の計 〜49人 は 55,408', () => {
    const html = render({ tab: 'stock' });
    const rows = tableRows(html, 'stock-bands-リクロジ');
    expect(rows[6]?.[0]).toBe('保管の計');
    // 〜49人: 53,324 + 2,084 + 0 = 55,408、全体 87,754 → 63.1%。名前あり 22,711 (Python で独立に集計)
    expect(rows[6]?.[2]).toBe('55,408全体の 63.1%名前あり 22,711件数の 41.0%');
    expect(tableRows(html, 'stock-bands-リクロジ', 'tfoot')[0]?.[7]).toBe('164,159名前あり 92,442件数の 56.3%');
  });
});
