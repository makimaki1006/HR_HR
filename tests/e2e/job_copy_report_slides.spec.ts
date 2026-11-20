import { expect, test } from '@playwright/test';
import type { Page } from '@playwright/test';
import { writeFile } from 'node:fs/promises';

// 架空の案件・求人・応募者だけを使う。応募データは 11/15 まで。MTG の記録は 9/21 と 10/19（今回 10/19〜11/15、前回 9/21〜10/18）。
const SHOTS = process.env.JOB_COPY_SHOTS_DIR;
const listing = (id: string, title: string, status: string) => ({ id, media: 'hrh', media_job_id: `9${id}`, account_id: 'synthetic-shop', title, prefecture: '神奈川県', municipality: '横浜市', category: 'ドライバー', publication_status: status, last_csv_detected_at: null, application_count: null, deal_ids: ['901'] });
const listings = [listing('301', '合成トラックドライバー', '公開中'), listing('302', '合成ルート配送', '非公開')];
const byDate: Record<string, Record<string, number>> = {
  301: { '2026-09-15': 1, '2026-09-25': 1, '2026-10-05': 1, '2026-10-20': 2, '2026-10-28': 1, '2026-11-03': 2 },
  302: { '2026-10-02': 1, '2026-10-25': 2 },
};
const people: Record<string, { date: string; gender: string; age: string; prefecture: string; municipality: string; text: string }[]> = {
  301: [
    { date: '2026-09-15', gender: '男性', age: '48', prefecture: '千葉県', municipality: '千葉市', text: '給与' },
    { date: '2026-09-25', gender: '男性', age: '50', prefecture: '千葉県', municipality: '千葉市', text: '給与が良いので' },
    { date: '2026-10-05', gender: '男性', age: '41', prefecture: '神奈川県', municipality: '横浜市', text: '家から近い' },
    { date: '2026-10-20', gender: '男性', age: '35', prefecture: '神奈川県', municipality: '横浜市', text: '給与が良いので' },
    { date: '2026-10-20', gender: '男性', age: '36', prefecture: '神奈川県', municipality: '横浜市', text: '給料が高い' },
    { date: '2026-10-28', gender: '男性', age: '38', prefecture: '神奈川県', municipality: '横浜市', text: '自宅から近い' },
    { date: '2026-11-03', gender: '女性', age: '25', prefecture: '東京都', municipality: '新宿区', text: '給与と、家から近いので' },
    { date: '2026-11-03', gender: '女性', age: '62', prefecture: '東京都', municipality: '渋谷区', text: '手当が多い' },
  ],
  302: [
    { date: '2026-10-02', gender: '男性', age: '44', prefecture: '神奈川県', municipality: '川崎市', text: '近所なので' },
    { date: '2026-10-25', gender: '男性', age: '31', prefecture: '神奈川県', municipality: '横浜市', text: '給与が良い' },
    { date: '2026-10-25', gender: '女性', age: '29', prefecture: '神奈川県', municipality: '横浜市', text: 'ボーナスがある' },
  ],
};
const key = (id: string, n: number) => `${id}${String(n)}`.padStart(64, 'a');
// 電話でうかがった応募理由の分類（ouboriyuukategori_hiaringu）。本文からの推定は報告には使わない。
const CATEGORY: Record<string, string> = { 給与が良いので: '給与', 給料が高い: '給与', 手当が多い: '給与', 給与が良い: '給与', ボーナスがある: '給与', 家から近い: '勤務地', 自宅から近い: '勤務地', 近所なので: '勤務地', '給与と、家から近いので': '給与' };
function applicants(id: string) {
  const rows = people[id] ?? [];
  const total = rows.length;
  const chosen = rows.map((row, n) => ({ applicant: key(id, n), source_property: 'ouboriyuukategori_hiaringu', value: CATEGORY[row.text] ?? 'その他', label: CATEGORY[row.text] ?? 'その他', application_date: row.date }));
  const sources = ['oubodouki', 'ouboriyuu_baitaikisai', 'ouboriyuu_hiaringu', 'genshokumaeshokukaranotenshokuriyuu', 'ouboriyuukategori_hiaringu', 'ouboriyuukategori_baitaikisai'];
  const nonblank: Record<string, number> = { ouboriyuu_hiaringu: total, ouboriyuukategori_hiaringu: total };
  const source_counts = Object.fromEntries(sources.map(name => [name, { missing: 0, blank: total - (nonblank[name] ?? 0), nonblank: nonblank[name] ?? 0 }]));
  return {
    listing_id: id, total, missing_date: 0, by_date: byDate[id], fetched_at: '2026-11-15T09:00:00+09:00',
    applications: rows.map((row, n) => ({ key: key(id, n), date: row.date, gender: row.gender, age: row.age, prefecture: row.prefecture, municipality: row.municipality })),
    applicant_reasons: {
      available: true, source: 'hubspot', basis: 'recorded_applicant_reason', source_property: null, fetched_at: '2026-11-15T09:00:00+09:00', total_applicants: total, total_source_values: total * 6, source_counts,
      items: rows.map((row, n) => ({ id: key(`r${id}`, n).replace(/[^a-f0-9]/g, 'b'), applicant: key(id, n), text: row.text, source: 'hubspot', source_property: 'ouboriyuu_hiaringu', application_date: row.date, collected_at: null, version_id: null })),
      selections: chosen, option_labels: 'read', multi_listing_applicants: [], missing: 0, blank: total * 4, truncated: false,
    },
  };
}

async function open(page: Page) {
  await page.route('**/api/crm/workspace/deals/**', route => route.fulfill({ json: { deal: { id: '901', name: '合成・定期採用' }, partial: [], selected: { deal: { mensetu: '4', syoudaku: '1' }, contacts: {}, company: {} } } }));
  await page.route('**/api/consulting/deal-detail**', route => route.fulfill({ json: { meta: { found: true }, events: [
    { kind: 'mtg', date: '2026-09-21', source_label: '架空の記録', fact: true }, { kind: 'zoom_mtg', date: '2026-10-19', source_label: '架空の記録', fact: true },
    { kind: 'email', date: '2026-11-02', source_label: '架空の記録', fact: true },
  ] } }));
  await page.route('**/api/job-copy/**', route => {
    const url = new URL(route.request().url());
    if (url.pathname.endsWith('/listings') && url.searchParams.get('deal') === '901') return route.fulfill({ json: { status: 'ready', total: 2, index_built_at: '2026-11-15T00:00:00Z', listings, titles: ['ドライバー'], offset: 0, next_offset: null, refreshing: false, refresh_failed: false, selected_deal: { id: '901', name: '合成・定期採用' } } });
    if (url.pathname.endsWith('/deal-applications')) return route.fulfill({ json: { listings: listings.map(row => ({ listing_id: row.id, total: (people[row.id] ?? []).length, by_date: byDate[row.id], fetched_at: '2026-11-15T09:00:00+09:00' })), shared_applications: 0, shared_by_date: {}, company: { id: '801', name: '合成運輸' } } });
    if (url.pathname.endsWith('/listing-body-dates')) return route.fulfill({ json: { dates: { 301: null, 302: null }, changes: {
      301: { body: ['2026-10-19T03:00:00Z'], body_first: '2026-08-01T00:00:00Z', images: ['2026-11-01T03:00:00Z'], status: {}, started_on: '2026-08-01' },
      302: { body: [], body_first: '2026-09-01T00:00:00Z', images: [], status: { baitai_genjoukyou_hrhakkaa: [{ timestamp: '2026-09-01T00:00:00Z', value: '公開' }, { timestamp: '2026-11-05T03:00:00Z', value: '非公開' }] }, started_on: '2026-09-01' },
    } } });
    const match = /\/listings\/(\d+)\/applicants$/.exec(url.pathname);
    if (match?.[1]) return route.fulfill({ json: applicants(match[1]) });
    return route.fulfill({ status: 503, json: { code: 'synthetic_unavailable' } });
  });
  await page.goto('/app/job-copy?report=deal:901');
}

const SHORT = '週1回、掲載の数字を確認します\n必要に応じて原稿を見直します';
/** 行動計画の3つの欄を書く（選んでいるのは「次回までの目標と行動計画」のスライド）。 */
async function fillPlan(page: Page, text: string) {
  for (const name of ['当社の行動計画', 'ご担当者さまの行動計画', '運用上のゴール']) await page.getByRole('textbox', { name, exact: true }).fill(text);
}

test('案件の報告のスライド: 左で選び、未記入・外したものは理由を出し、PDF は横長1枚ずつ', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await open(page);
  const side = page.getByRole('complementary', { name: '報告の設定' });
  const stage = page.getByRole('region', { name: 'スライド', exact: true });
  const printed = page.locator('.rs-print-deck');
  // 定期ミーティングの資料の並び: 表紙・目次・掲載求人・導線・進捗・数字の表・総評・求人ごと2枚・補足4枚・対応方針・施策2枚・行動計画・アンケート
  await expect(side.locator('.rs-thumbs').getByRole('checkbox')).toHaveCount(18);
  await expect(side.getByRole('checkbox', { name: /面接・採用を出す/ })).toBeChecked();
  // 期間は前回の MTG の記録から決まる
  await expect(side).toContainText('前回のMTG（10/19 の記録）');
  await expect(side.getByLabel('開始', { exact: true })).toHaveValue('2026-10-19');
  await expect(side.getByLabel('終了', { exact: true })).toHaveValue('2026-11-15');
  await expect(side.getByLabel('前回の開始')).toHaveValue('2026-09-21');
  await expect(side.getByLabel('前回の終了')).toHaveValue('2026-10-18');
  // 文章が未記入のスライドと補足のスライドは、最初は紙に入らない
  await expect(page.getByRole('region', { name: '外したもの' })).toContainText('次回までの目標と行動計画：共通目標・行動計画が未記入のため');
  await expect(printed.locator('.rs-slide')).toHaveCount(9);
  await expect(printed.locator('[data-slide="plan"], [data-slide="trend"], [data-slide="reasons"]')).toHaveCount(0);
  await side.getByRole('button', { name: /次回までの目標と行動計画/ }).click();
  await page.getByRole('textbox', { name: '次回の定期MTGまでの共通目標', exact: true }).fill('内定 1名の獲得');
  await fillPlan(page, SHORT);
  await expect(side.locator('.rs-thumbs').getByRole('checkbox')).toHaveCount(18);
  await expect(printed.locator('.rs-slide')).toHaveCount(10);
  await expect(printed.locator('[data-slide="plan"]')).toContainText('内定 1名の獲得');
  // 数字の表: 求人ごとの応募数と合計。面接・採用は期間の数字とは別の段に、案件を読んだ日つきの見出しで出す（2026-11-16 利用者決定）
  await side.getByRole('button', { name: /数字の表/ }).click();
  await expect(stage.locator('.rs-table tbody tr')).toHaveText(['合成トラックドライバー5', /^合成ルート配送.*2$/, '合計7']);
  const today = new Intl.DateTimeFormat('en-US', { timeZone: 'Asia/Tokyo', month: 'numeric', day: 'numeric' }).format(new Date());
  await expect(stage.locator('.rs-totals-h')).toHaveText(`ご契約からの合計（${today} 時点）`);
  await expect(stage.locator('.rs-totals .rs-kp > div')).toHaveText(['面接4件', '採用1名']);
  await expect(stage.locator('.rs-table')).not.toContainText(/面接|採用/);
  await expect(stage).not.toContainText(/未取得|HubSpot/);
  // 前回の MTG の期間と並べる（全体は 前回 3件 → 今回 7件）
  await side.getByRole('button', { name: /総評と前回との比較/ }).click();
  await expect(stage.locator('.rs-compare tbody tr').first()).toHaveText('全体3 → 7');
  await expect(stage).not.toContainText(/未取得|HubSpot/);
  // 媒体の費用は紙のどこにも書かない（印刷用の束の全スライド）
  await expect(printed).not.toContainText(/費用|課金|円/);
  // 切り替えで面接・採用を外せる
  await side.getByRole('checkbox', { name: /面接・採用を出す/ }).uncheck();
  await expect(printed.locator('.rs-totals')).toHaveCount(0);
  await expect(printed).not.toContainText(/面接\d|採用\d|ご契約からの合計/);
  await side.getByRole('checkbox', { name: /面接・採用を出す/ }).check();
  await expect(printed.locator('.rs-totals .rs-kp > div')).toHaveText(['面接4件', '採用1名']);
  // 補足の応募理由は左で選ぶと入る。電話でうかがった分類の件数、どのスライドも枠からはみ出さない
  await side.getByRole('checkbox', { name: '応募理由を入れる' }).check();
  await expect(printed.locator('.rs-slide')).toHaveCount(11);
  await side.getByRole('button', { name: /応募理由/ }).click();
  await expect(stage).toContainText('お電話で応募理由をうかがえた 7件の内訳');
  await expect(page.getByRole('region', { name: 'はみ出し' })).toHaveCount(0);
  // 長すぎる文は、はみ出しとして左に出る（紙は直さず、担当者が短くする）
  await side.getByRole('button', { name: /次回までの目標と行動計画/ }).click();
  await fillPlan(page, Array.from({ length: 25 }, () => '長い文の確認です。長い文の確認です。').join('\n'));
  await expect(page.getByRole('region', { name: 'はみ出し' })).toContainText('次回までの目標と行動計画（枠からはみ出しています）');
  await fillPlan(page, SHORT);
  await expect(page.getByRole('region', { name: 'はみ出し' })).toHaveCount(0);
  // 期間が正しくないときは PDF に出せない
  await side.getByLabel('開始', { exact: true }).fill('2026-12-01');
  await expect(page.getByRole('button', { name: 'PDF に出す' })).toBeDisabled();
  await side.getByLabel('開始', { exact: true }).fill('2026-10-19');
  await expect(page.getByRole('button', { name: 'PDF に出す' })).toBeEnabled();
  if (SHOTS) await page.screenshot({ path: `${SHOTS}/report-slides-screen.png` });
  if (SHOTS) for (let n = 0; n < 18; n += 1) {
    await side.locator('.rs-thumbs li').nth(n).getByRole('button').click();
    await page.locator('.rs-big .rs-slide').screenshot({ path: `${SHOTS}/slide-${String(n + 1).padStart(2, '0')}.png` });
  }
  // 入れるスライドを外すと、印刷にも出ない。
  await side.getByRole('checkbox', { name: '応募理由を入れる' }).uncheck();
  await expect(printed.locator('.rs-slide')).toHaveCount(10);
  await side.getByRole('checkbox', { name: '応募理由を入れる' }).check();
  // 印刷では画面の他の部分を出さず、スライドだけを横長1枚ずつ。
  await page.emulateMedia({ media: 'print' });
  await expect(page.locator('.rs-root')).toBeHidden();
  await expect(printed.locator('.rs-slide').first()).toBeVisible();
  const pdf = await page.pdf({ preferCSSPageSize: true, printBackground: true });
  const pages = pdf.toString('latin1').match(/\/Type\s*\/Page[^s]/g) ?? [];
  expect(pages).toHaveLength(11);
  const box = pdf.toString('latin1').match(/\/MediaBox\s*\[\s*0\s+0\s+([\d.]+)\s+([\d.]+)\s*\]/);
  // 10in × 5.625in = 720pt × 405pt（16:9 の横長）
  expect(Number(box?.[1])).toBeCloseTo(720, 0);
  expect(Number(box?.[2])).toBeCloseTo(405, 0);
  if (SHOTS) await writeFile(`${SHOTS}/report-slides.pdf`, pdf);
});
