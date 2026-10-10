import { expect, test, type Page } from '@playwright/test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import fixtures from '../../../frontend/src/generated/jobgen/fixtures.json';
import { login } from './helpers/login';

const root = path.resolve(__dirname, '../../..');
const sample = (name: string) => path.join(root, 'tests/fixtures/jobgen', name);
const columns = Object.keys(fixtures.responses.hrhacker.row);

// 取り込みだけは本物の Rust API。生成 API は全て遮断し、架空の応答に置換する。
async function mockGeneration(page: Page) {
  await page.route('**/api/jobgen/*', async (route) => {
    const key = new URL(route.request().url()).pathname.split('/').pop()!;
    if (key === 'normalize') return route.continue();
    const responses = fixtures.responses as Record<string, unknown>;
    if (!responses[key]) throw new Error(`未定義の生成 API: ${key}`);
    const body = route.request().postDataJSON();
    let response = structuredClone(responses[key]) as Record<string, unknown>;
    const salary = (body.source_text as string | undefined)?.match(/月給[\d,]+円|250000 yen\/month/)?.[0] ?? '月給250,000円';
    const hours = (body.source_text as string | undefined)?.match(/9:00[〜～]18:00/)?.[0] ?? '';
    const people = ['倉庫の仕事を始めたい人', '検品の経験を活かしたい人', '勤務条件を比較したい人'].map(label => ({ label, profile: '架空の求職者像', dissatisfaction: '希望する勤務条件を確認したい', environment: '物流の仕事を検討中', pain: '仕事内容と条件が分からないと応募を決めにくい' }));
    const checked = { status: 'ok', ng_violations: [], expression_warnings: [], number_violations: [], number_check: 'checked', review_required: false };
    if (key === 'analyze') response = { status: 'ok', category: '物流', analysis: { surface_strengths: ['資料に給与と勤務時間の記載がある'], hidden_strengths: ['検品と梱包の業務が記載されている'], bottlenecks: ['必須資格は未取得のため確認が必要'] }, knowledge_used: false };
    if (key === 'personas') response = { status: 'ok', personas: people };
    if (key === 'copy') response = { ...checked, copies: [{ style: '仕事内容', text: '商品の検品と梱包を担当する仕事です。' }] };
    if (key === 'images') response = { ...checked, directions: people.map(p => ({ persona_label: p.label, direction: '検品と梱包の作業が分かる写真。架空の画像案です。' })) };
    if (key === 'image_prompts') response = { ...checked, prompts: people.map(p => ({ persona_label: p.label, appeal_core: '作業内容を具体的に伝える', prompt: '明るい倉庫で商品を検品する場面', negative_prompt: '個人情報を入れない', aspect_ratio: '4:5' })) };
    if (key === 'mobile') response = { ...checked, lines: ['商品の検品と梱包を担当します。', '', `給与は${salary}です。`] };
    if (key === 'ab') response = { ...checked, steps: [{ metric: '応募数', action: '掲載期間と閲覧数も確認しながら比較してください。文面が応募の変化の原因とは断定できません。' }] };
    if (key === 'extract') {
      response = { status: 'ok', facts_text: `給与: ${salary}`, facts: {
        salary: { value: salary, evidence_quote: salary, status: 'verified' },
        working_hours: { value: hours, evidence_quote: hours, status: hours ? 'verified' : 'missing' },
        required_qualifications: { value: '', evidence_quote: '', status: 'missing' },
      } };
    }
    if (key === 'hrhacker') {
      const row = Object.fromEntries(columns.map(c => [c, '']));
      row['案件名'] = '架空の倉庫スタッフ';
      row['仕事内容'] = '商品の検品、梱包\n商品に「傷」がないか確認';
      row['給与補足'] = salary;
      row['求人id'] = 'SAMPLE-PRIVATE-001';
      const generated_fields = Object.fromEntries([
        ['job_title', '案件名', row['案件名']],
        ['job_description', '仕事内容', row['仕事内容']],
      ].map(([key, column, value]) => [key, { column, value, status: 'generated_verified', issues: [] }]));
      response = { status: 'ok', row, generated_fields, review_required_fields: [], unsupported_numbers: [], fill_stats: null, unassigned_hints: [] };
    }
    await route.fulfill({ json: response });
  });
}

async function upload(page: Page, kind: string, file: string) {
  await page.locator(`[data-kind="${kind}"]`).click();
  await page.locator('#fileInput').setInputFiles(sample(file));
  await page.locator('#normBtn').click();
  await expect(page.locator('#normBtn')).toBeEnabled();
}

for (const width of [1440, 1920]) {
  test.describe(`求人票作成 ${width}`, () => {
    test.beforeEach(async ({ page }) => {
      await page.setViewportSize({ width, height: 1000 });
      await login(page);
      await mockGeneration(page);
      await page.goto('/app/jobgen');
    });

    for (const input of [
      { kind: 'csv', file: 'customer-utf8.csv', title: '倉庫スタッフ', salary: '月給250,000円', multi: true },
      { kind: 'csv', file: 'customer-sjis.csv', title: '倉庫スタッフ', salary: '月給250,000円', multi: true },
      { kind: 'excel', file: 'customer.xlsx', title: '倉庫スタッフ', salary: '月給250,000円', multi: true },
      { kind: 'pdf', file: 'customer-text.pdf', title: 'Warehouse staff', salary: '250000 yen/month', multi: false },
      { kind: 'free_text', file: 'customer.txt', title: '職種名: 倉庫スタッフ', salary: '月給250,000円', multi: false },
    ]) {
      test(`${input.file}: 取り込みから84列の出力まで`, async ({ page }) => {
        if (input.kind === 'free_text') {
          await page.locator('#freeText').fill(fs.readFileSync(sample(input.file), 'utf8'));
          await page.locator('#normBtn').click();
        } else await upload(page, input.kind, input.file);
        if (input.multi) {
          await expect(page.locator('.jobitem')).toHaveCount(2);
          if (width === 1440 && input.file === 'customer-utf8.csv' && process.env.JOBGEN_SCREENSHOTS) {
            const dir = path.join(root, 'docs/screenshots/jobgen-usability');
            fs.mkdirSync(dir, { recursive: true });
            await page.screenshot({ path: path.join(dir, 'import-1440.png') });
          }
          await page.locator('.jobitem').first().click();
        }
        await expect(page.locator('#jobTitle')).toHaveValue(input.title);
        await page.locator('#jobConfirmChk').check();
        await page.locator('#personaCount').selectOption('3');
        await page.locator('#runAllBtn').click();
        await expect(page.locator('#csvBtn')).toBeVisible();
        await expect(page.locator('#res-extract')).toContainText(input.salary);
        if (input.multi) await expect(page.locator('#res-extract')).toContainText(input.file === 'customer-sjis.csv' ? '9:00～18:00' : '9:00〜18:00');
        await expect(page.locator('#res-extract')).toContainText('未取得');
        await expect(page.locator('#res-hrhacker')).not.toContainText('SAMPLE-PRIVATE-001');
        const downloadPromise = page.waitForEvent('download');
        await page.locator('#csvBtn').click();
        const download = await downloadPromise;
        const bytes = fs.readFileSync((await download.path())!);
        expect([...bytes.subarray(0, 3)]).toEqual([239, 187, 191]);
        expect(bytes.toString('utf8').split('\r\n')[0]).toBe('\uFEFF' + columns.join(','));
        expect(bytes.toString('utf8')).toContain(input.salary);
        expect(bytes.toString('utf8')).toContain('"商品の検品、梱包\n商品に「傷」がないか確認"');
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
        if (width === 1440 && input.file === 'customer-utf8.csv' && process.env.JOBGEN_SCREENSHOTS) {
          const dir = path.join(root, 'docs/screenshots/jobgen-usability');
          fs.mkdirSync(dir, { recursive: true });
          await page.locator('#res-extract').evaluate(el => el.scrollIntoView({ block: 'start' }));
          await page.screenshot({ path: path.join(dir, 'facts-1440.png') });
          await page.locator('#reviewFilter').selectOption('filled');
          await page.locator('#res-hrhacker').evaluate(el => el.scrollIntoView({ block: 'start' }));
          await page.screenshot({ path: path.join(dir, 'review-1440.png') });
        }
      });
    }

    test('画像だけのPDFは代わりの入力方法を案内する', async ({ page }) => {
      await upload(page, 'pdf', 'customer-image.pdf');
      await expect(page.locator('#status')).toContainText('文字の入ったPDF');
      await expect(page.locator('#runAllBtn')).toBeDisabled();
    });

    test('複数求人の取り込み直しで前の求人と出力を残さない', async ({ page }) => {
      await page.locator('#freeText').fill('営業\n月給400,000円');
      await page.locator('#normBtn').click();
      await page.locator('#jobConfirmChk').check();
      await page.locator('[data-rerun="extract"]').click();
      await expect(page.locator('#res-extract')).toBeVisible();
      await page.locator('[data-rerun="hrhacker"]').click();
      await expect(page.locator('#csvBtn')).toBeVisible();
      await upload(page, 'csv', 'customer-utf8.csv');
      await expect(page.locator('#runAllBtn')).toBeDisabled();
      await expect(page.locator('#csvBtn')).toBeHidden();
      await page.locator('.jobitem').nth(1).click();
      await expect(page.locator('#jobTitle')).toHaveValue('配送スタッフ');
      await page.locator('[data-rerun="extract"]').click();
      await expect(page.locator('#res-extract')).toContainText('月給300,000円');
    });

    test('列数が違うCSVは値を捨てず取り込みを止める', async ({ page }) => {
      await upload(page, 'csv', 'customer-invalid.csv');
      await expect(page.locator('#status')).toContainText('2行目');
      await expect(page.locator('#status')).toContainText('項目数');
      await expect(page.locator('#runAllBtn')).toBeDisabled();
    });

    test('84項目は入力済み・未取得・項目名で絞れるが出力は84列を保つ', async ({ page }) => {
      await page.locator('#freeText').fill('倉庫スタッフ\n月給250,000円');
      await page.locator('#normBtn').click();
      await page.locator('#jobConfirmChk').check();
      await page.locator('#runAllBtn').click();
      await expect(page.locator('#csvBtn')).toBeVisible();
      await page.locator('#reviewFilter').selectOption('filled');
      await expect(page.locator('#reviewTable tbody tr')).toHaveCount(4);
      await page.locator('#reviewSearch').fill('給与');
      await expect(page.locator('#reviewTable tbody tr')).toHaveCount(1);
      await expect(page.locator('#reviewTable')).toContainText('月給250,000円');
      await page.locator('#reviewSearch').fill('');
      await page.locator('#reviewFilter').selectOption('missing');
      await expect(page.locator('#reviewTable tbody tr')).toHaveCount(80);
      const promise = page.waitForEvent('download');
      await page.locator('#csvBtn').click();
      const d = await promise;
      expect(fs.readFileSync((await d.path())!, 'utf8').split('\r\n')[0].split(',')).toHaveLength(84);
    });

    test('待ち時間を表示し、作成中は資料と対象求人を変更できない', async ({ page }) => {
      await upload(page, 'csv', 'customer-utf8.csv');
      await page.locator('.jobitem').first().click();
      await page.locator('#jobConfirmChk').check();
      let release!: () => void;
      const hold = new Promise<void>(resolve => { release = resolve; });
      await page.route('**/api/jobgen/extract', async route => {
        await hold;
        await route.fulfill({ json: fixtures.responses.extract });
      });
      try {
        await page.locator('[data-rerun="extract"]').click();
        await expect(page.locator('#status')).toContainText('1秒経過');
        await expect(page.locator('#normBtn')).toBeDisabled();
        await expect(page.locator('.jobitem').nth(1)).toBeDisabled();
        await expect(page.locator('#jobTitle')).toBeDisabled();
        await expect(page.locator('[data-kind="excel"]')).toBeDisabled();
        if (width === 1440 && process.env.JOBGEN_SCREENSHOTS) {
          await page.locator('#status').evaluate(el => el.scrollIntoView({ block: 'center' }));
          await page.screenshot({ path: path.join(root, 'docs/screenshots/jobgen-usability/waiting-1440.png') });
        }
      } finally { release(); }
      await expect(page.locator('#normBtn')).toBeEnabled();
    });
  });
}
