import { test, expect, type Page } from '@playwright/test';
import { login } from './helpers/login';
import fixtures from '../../../frontend/src/generated/jobgen/fixtures.json';
import columns from '../../../frontend/src/generated/jobgen/columns.json';
import fs from 'node:fs';
import path from 'node:path';
const directory = path.resolve('docs/screenshots/jobgen-to-job-copy');
const source = '架空配送スタッフ\n給与は月給270,000円〜300,000円です。\n勤務地は大分県大分市です。\n土日休みです。';
const facts = { salary:{value:'月給270,000円〜300,000円',evidence_quote:'給与は月給270,000円〜300,000円です。',status:'verified'},work_location:{value:'大分県大分市',evidence_quote:'勤務地は大分県大分市です。',status:'verified'},holidays:{value:'土日休み',evidence_quote:'土日休みです。',status:'verified'} };
const row = {...Object.fromEntries(columns.map(column => [column,''])), '案件名':'架空配送スタッフ','仕事内容':'日用品を配送します。','給与形態':'月給','基本給与 最小':'270000','基本給与 最大':'300000','求人id':'hidden-job-123','店舗id':'hidden-shop-456','職種id':'hidden-role-42'};
function draft(id = '10000000-0000-4000-8000-000000000001', date = '2026-10-11T00:00:00Z', status = 'pending') { return {schema_version:1,draft_id:id,created_at:date,source_kind:'free_text',review_status:status,row,facts}; }
async function mock(page: Page, options: { allowed?: boolean; queued?: boolean } = {}) {
  const writes: { method: string; body: any }[] = [];
  let latest: ReturnType<typeof draft> | null = null;
  const past = draft('20000000-0000-4000-8000-000000000002','2026-10-10T00:00:00Z','rejected');
  let revision = 'a'.repeat(64);
  let operation = '';
  const listing = {id:'30',media:'hrh',media_job_id:'hidden-hrh-999',account_id:null,title:'架空配送スタッフ',prefecture:'大分県',municipality:'別府市',category:'配送ドライバー',publication_status:'掲載中',last_csv_detected_at:null,application_count:null};
  const published = '案件名：架空配送スタッフ\n仕事内容：日用品を配送します。\n給与形態：月給\n基本給与 最小：250000\n基本給与 最大：280000\n自由項目1のタイトル：休日\n自由項目1の内容：土日休み';
  await page.route('**/api/jobgen/*', route => {
    const step = new URL(route.request().url()).pathname.split('/').at(-1) ?? '';
    const values: Record<string, unknown> = {...fixtures.responses,normalize:{status:'ok',jobs:[{title_hint:'配送ドライバー',source_text:source}]},extract:{...fixtures.responses.extract,facts},hrhacker:{...fixtures.responses.hrhacker,row}};
    return route.fulfill({json:values[step]});
  });
  await page.route('**/api/job-copy/listings*', route => route.fulfill({json:{status:'ready',listings:[listing],titles:['配送ドライバー'],offset:0,next_offset:null,refreshing:false,refresh_failed:false,total:1,index_built_at:'2026-10-11T00:00:00Z'}}));
  await page.route('**/api/job-copy/listings/30/versions', route => route.fulfill({json:{listing,versions:[{written_at:'2026-10-01T00:00:00Z',body:published,image_urls:null}],history_counts:{hrh_kyuujinhyou_honbun:1},history_may_be_incomplete:false,drafts:latest ? [past,latest] : [past],draft_revision:revision,can_write_drafts:options.allowed !== false}}));
  await page.route('**/api/job-copy/listings/30/draft', route => {
    const body = route.request().postDataJSON(); const method = route.request().method(); writes.push({method,body});
    if (options.allowed === false) return route.fulfill({status:403,json:{code:'draft_writes_disabled'}});
    if (method === 'POST') { latest = {...draft(body.operation_id),row:body.row,facts:body.facts,source_kind:body.source_kind}; } else if (latest) latest.review_status = body.status;
    revision = 'b'.repeat(64); operation = body.operation_id;
    return route.fulfill({status:options.queued ? 202 : 200,json:{status:options.queued ? 'pending':'saved',operation_id:operation,draft:options.queued ? null:latest,revision:options.queued ? null:revision,code:''}});
  });
  await page.route('**/api/job-copy/draft-operations/*', route => route.fulfill({json:{status:'saved',operation_id:operation,draft:latest,revision,code:''}}));
  return { writes };
}
async function generate(page: Page) {
  await page.goto('/app/jobgen'); await page.locator('#freeText').fill(source); await page.locator('#normBtn').click(); await expect(page.locator('#jobTitle')).toHaveValue('配送ドライバー'); await page.locator('#jobConfirmChk').check(); await page.locator('#runAllBtn').click();
  await expect(page.locator('#csvBtn')).toBeVisible(); await expect(page.locator('#runAllBtn')).toBeEnabled(); await page.getByRole('button',{name:'保存先の求人を選ぶ'}).click();
}
for (const width of [1440,1920]) test.describe(`求人票作成から案の比較 ${width}`, () => {
  test.use({viewport:{width,height:1100}});
  test('案を明示保存し、給与・文字色・事実の違い・過去の案・確認状態を確認', async ({page}) => {
    const {writes} = await mock(page); await login(page); await generate(page); expect(writes).toHaveLength(0);
    await page.getByRole('button',{name:'架空配送スタッフの版を見る'}).click(); await expect(page.getByRole('button',{name:'この求人に案を保存'})).toBeEnabled();
    await page.getByRole('button',{name:'この求人に案を保存'}).click(); await expect(page.getByRole('link',{name:'求人文面管理で今の版と案を比べる'})).toBeVisible();
    expect(writes).toHaveLength(1); expect(writes[0].body).toMatchObject({source_kind:'free_text',source_text:source,row:{'基本給与 最小':'270000','基本給与 最大':'300000'},facts}); expect(Object.keys(writes[0].body.row)).toHaveLength(84);
    if (width === 1440) { fs.mkdirSync(directory,{recursive:true}); await page.getByRole('heading',{name:'作った案を求人に保存'}).scrollIntoViewIfNeeded(); await page.screenshot({path:path.join(directory,'01-save-1440.png')}); }
    await page.getByRole('link',{name:'求人文面管理で今の版と案を比べる'}).click(); await expect(page.locator('.jc-comparison')).toBeVisible();
    await expect(page.getByLabel('比較元', {exact:true})).toHaveValue('hubspot-history-30-0'); await expect(page.getByLabel('比較先',{exact:true})).toHaveValue(`hubspot-draft-${writes[0].body.operation_id}`);
    await expect(page.locator('.jc-draft-facts')).toContainText('月給 250,000円〜280,000円'); await expect(page.locator('.jc-draft-facts')).toContainText('月給270,000円〜300,000円'); await expect(page.locator('.jc-draft-facts')).toContainText('大分県別府市'); await expect(page.locator('.jc-draft-facts')).toContainText('大分県大分市');
    await expect(page.locator('.jc-mark-added').first()).toBeVisible(); await expect(page.locator('.jc-mark-removed').first()).toBeVisible();
    if (width === 1440) { await page.locator('#job-copy-text-diff').scrollIntoViewIfNeeded(); await page.screenshot({path:path.join(directory,'03-text-diff-1440.png')}); }
     const diffText = await page.locator('.jc-diff-lines').innerText(); expect(diffText).toContain('270,000円'); expect(diffText).not.toMatch(/hidden-|基本給与|給与形態|求人id|店舗id/);
    if (width === 1440) { await page.locator('.jc-draft-review').scrollIntoViewIfNeeded(); await page.screenshot({path:path.join(directory,'02-compare-1440.png')}); }
    await page.getByLabel('比較先',{exact:true}).selectOption('hubspot-draft-20000000-0000-4000-8000-000000000002'); await expect(page.locator('.jc-draft-review')).toContainText('過去の案です'); await expect(page.getByRole('button',{name:'確認状態を保存'})).toHaveCount(0);
    await page.getByLabel('比較先',{exact:true}).selectOption(`hubspot-draft-${writes[0].body.operation_id}`); await page.getByRole('combobox',{name:'案の確認状態',exact:true}).selectOption('adopted'); await page.getByRole('button',{name:'確認状態を保存'}).click(); await expect(page.locator('.jc-draft-review h3')).toHaveText('採用の案'); expect(writes).toHaveLength(2); expect(writes[1].method).toBe('PATCH'); expect(writes[1].body).toMatchObject({draft_id:writes[0].body.operation_id,status:'adopted',base_revision:'b'.repeat(64)});
    await expect(page.locator('.jc-diff-lines')).toContainText('250,000円'); await page.reload(); await expect(page.locator('.jc-draft-review h3')).toHaveText('採用の案');
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth); expect(overflow).toBe(false);
  });
  test('権限・設定がない求人には保存できない', async ({page}) => { const {writes} = await mock(page,{allowed:false}); await login(page); await generate(page); await page.getByRole('button',{name:'架空配送スタッフの版を見る'}).click(); await expect(page.getByRole('button',{name:'この求人に案を保存'})).toBeDisabled(); await expect(page.getByText('案を保存する権限または利用設定がありません。')).toBeVisible(); expect(writes).toHaveLength(0); });
  test('再送待ちの結果を照会し、保存完了まで待つ', async ({page}) => { const {writes} = await mock(page,{queued:true}); await login(page); await generate(page); await page.getByRole('button',{name:'架空配送スタッフの版を見る'}).click(); await page.getByRole('button',{name:'この求人に案を保存'}).click(); await expect(page.getByText('案の保存を受け付けました。時間を置いて自動で再確認します。')).toBeVisible(); await expect(page.getByRole('button',{name:'この求人に案を保存'})).toBeDisabled(); await expect(page.getByRole('link',{name:'求人文面管理で今の版と案を比べる'})).toBeVisible(); expect(writes).toHaveLength(1); });
});
