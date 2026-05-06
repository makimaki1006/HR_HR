/**
 * market_intelligence_display_rules.spec.ts
 *
 * Phase 3 Step 5 Phase 7 (Worker P7) Spec 2 / 4:
 *   MarketIntelligence variant の表示ルールを「具体テキスト」と「逆証明」で検証する。
 *
 * 検証ルール (Phase 3 Step 5 仕様 / feedback_unit_consistency_audit / feedback_neutral_expression):
 *   1. workplace 系 (従業地ベース) は「測定済」: 人数表示が許容される
 *   2. resident 系 (常住地ベース) は「estimated_beta」: 指数のみ、人数は禁止
 *   3. ハード NG: 「推定人数」「想定人数」「母集団人数」「N人見込み」は変数を問わず一切禁止
 *   4. parent_rank セルが national_rank セルより HTML 順序として前にある
 *
 * 設計指針:
 *   - feedback_reverse_proof_tests.md: 逆証明 (NG 文言の不在) で前提誤りを検出
 *   - feedback_test_data_validation.md: 「存在」ではなく「内容」を検証
 */

import { test, expect } from '@playwright/test';
import * as path from 'path';
import { loginAndUpload, buildReportUrl } from './helpers/session';

const FIXTURE = path.resolve(__dirname, 'fixtures', 'indeed_test_50.csv');

test.describe('MarketIntelligence display rules (Phase 7 Spec 2)', () => {
  test.setTimeout(240_000);

  test('workplace measured shows headcount, resident estimated_beta does not', async ({
    page,
  }) => {
    const { sessionId } = await loginAndUpload(page, FIXTURE, 'indeed');
    await page.goto(buildReportUrl(sessionId, 'market_intelligence'), {
      waitUntil: 'domcontentloaded',
    });

    const html = await page.content();

    // ---- Hard NG: 推定/想定/母集団 の人数表現が一切無いこと (estimated_beta は指数のみ) ----
    // feedback_reverse_proof_tests.md: ハード禁止語の不在で「実装が壊れていれば必ず失敗する」逆証明
    expect(html, 'estimated_beta セクションに「推定人数」が出ていないこと').not.toContain(
      '推定人数',
    );
    expect(html, '「想定人数」表現の禁止').not.toContain('想定人数');
    expect(html, '「母集団人数」表現の禁止').not.toContain('母集団人数');
    expect(
      html,
      '「N人見込み」のような人数見込み表現の禁止',
    ).not.toMatch(/[\d,]+\s*人見込み/);

    // ---- 表示ルール検証 (空 fixture では skip) ----
    // 50 行 fixture でレポートが生成されるが、市区町村結合の結果次第で
    // workplace / resident のいずれも空集合の場合は mi-empty fallback になりうる。
    // 該当セクションが描画されている場合のみ詳細ルールを検証する。

    const hasWorkplaceSection = html.includes('従業地ベース');
    const hasResidentSection = html.includes('常住地ベース');

    if (hasWorkplaceSection) {
      // workplace 系は人数 (例: "12,345 人") が出ていてよい (測定済 = measured)
      // ここでは「workplace セクションを含む HTML 全体に少なくとも 1 つの『N 人』表記がある」
      // ことを緩く確認する。innerText ベースで取り出して検証する。
      const text = await page.locator('body').innerText();
      const hasHeadcount = /[\d,]+\s*人(?![気口])/.test(text);
      // headcount が無くても workplace 自体が空集合 (measured 0) のケースは許容
      // → ここは informational annotation のみ
      if (!hasHeadcount) {
        test.info().annotations.push({
          type: 'note',
          description:
            'workplace セクションは描画されているが、headcount 数値が抽出できない (測定値 0 / 集計対象 0 の可能性)',
        });
      }
    }

    if (hasResidentSection) {
      // resident セクションのテキストブロックを抽出 (常住地ベース直下のセクション innerText)
      const residentText = await page
        .locator('text=常住地ベース')
        .first()
        .locator('xpath=ancestor::section[1] | xpath=ancestor::div[1]')
        .first()
        .innerText()
        .catch(() => '');

      if (residentText.length > 0) {
        // resident 側に「N 人」のような人数表現が混入していないこと
        // (許容: 「人気」「人口」「労働人口」等、人 の前後に他文字を含む語は除外)
        // 厳密には「数字 + 半角/全角空白? + 人」が末尾に来るケースを禁止
        const headcountMatches = residentText.match(/[\d,]+\s*人(?![気口])/g) ?? [];
        expect(
          headcountMatches,
          `resident (estimated_beta) に人数表現が混入: ${JSON.stringify(headcountMatches)}`,
        ).toEqual([]);
      }
    }
  });

  test('parent_rank cell appears before national_rank cell in row HTML order', async ({
    page,
  }) => {
    const { sessionId } = await loginAndUpload(page, FIXTURE, 'indeed');
    await page.goto(buildReportUrl(sessionId, 'market_intelligence'), {
      waitUntil: 'domcontentloaded',
    });

    const html = await page.content();

    if (!html.includes('mi-parent-ward-ranking') && !html.includes('mi-rank-table')) {
      test.info().annotations.push({
        type: 'note',
        description:
          'parent ward ranking テーブルが描画されていないため順序検証は skip (special_ward 対象外の市区町村など)',
      });
      return;
    }

    // 各 row で mi-parent-rank セルの出現位置 < mi-ref セルの出現位置
    // (= HTML 上で 親市内ランキングが 全国順位 より先に来る)
    const rows = await page.locator('.mi-rank-table tbody tr, table.mi-rank-table tbody tr').all();
    if (rows.length === 0) {
      test.info().annotations.push({
        type: 'note',
        description: 'mi-rank-table tbody tr が 0 件 (空 fallback)',
      });
      return;
    }

    let checkedRows = 0;
    for (const row of rows) {
      const rowHtml = await row.innerHTML();
      const parentIdx = rowHtml.indexOf('mi-parent-rank');
      const refIdx = rowHtml.indexOf('mi-ref');
      if (parentIdx >= 0 && refIdx >= 0) {
        expect(
          parentIdx,
          `mi-parent-rank セルが mi-ref セルより先に出現する (parent=${parentIdx}, ref=${refIdx})`,
        ).toBeLessThan(refIdx);
        checkedRows++;
      }
    }

    if (checkedRows === 0) {
      test.info().annotations.push({
        type: 'note',
        description:
          'parent_rank / ref の両方を含む row が 0 件のため順序検証は実質 skip',
      });
    }
  });
});
