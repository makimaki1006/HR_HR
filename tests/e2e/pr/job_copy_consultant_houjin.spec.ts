import { expect, test } from "@playwright/test";
import { login } from "./helpers/login";
import { fixtureJobs, fixtureHistory } from "../job-copy-listings-fixture.mjs";
import { mkdir } from "node:fs/promises";
import {
  houjinFixture,
  memberHoujinFixture,
} from "../../../frontend/src/screens/job-copy/houjin.testFixtures";
import {
  boardFixture,
  boardRows,
} from "../../../frontend/src/screens/job-copy/dealBoard.testFixtures";
import {
  scopeFixture,
  riskFixture,
  goodFixture,
  detailFixture,
} from "../../../frontend/src/screens/job-copy/consultantRisk.testFixtures";
test("法人の値と図・採用単価の並び・4メニューの担当引き継ぎ", async ({
  page,
}, testInfo) => {
  const shots = process.env.CONSULTANT_SCREENSHOT_DIR ?? testInfo.outputPath("houjin");
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.stack ?? e.message));
  const owner = (url: string) =>
    new URL(url).searchParams.get("owner") === "22";
  await page.route("**/api/job-copy/consultant-scope?*", (r) =>
    r.fulfill({
      json: { ...scopeFixture, owner: owner(r.request().url()) ? "22" : "11" },
    }),
  );
  await page.route("**/api/job-copy/houjin?*", (r) =>
    r.fulfill({
      json: owner(r.request().url()) ? memberHoujinFixture : new URL(r.request().url()).searchParams.get("houjin") === "sample-b" ? { ...memberHoujinFixture, index: houjinFixture.index } : houjinFixture,
    }),
  );
  await page.route("**/api/job-copy/deals?*", (r) =>
    r.fulfill({
      json: {
        ...boardFixture,
        work: { ...boardFixture.work, owner: owner(r.request().url()) ? "22" : "11" },
        rows: owner(r.request().url()) ? boardRows.slice(1, 2) : boardRows,
      },
    }),
  );
  await page.route("**/api/job-copy/consultant-risk?*", (r) =>
    r.fulfill({
      json: {
        ...riskFixture,
        rows: owner(r.request().url()) ? [] : riskFixture.rows,
      },
    }),
  );
  await page.route("**/api/job-copy/consultant-good?*", (r) =>
    r.fulfill({
      json: {
        ...goodFixture,
        rows: owner(r.request().url()) ? [] : goodFixture.rows,
      },
    }),
  );
  await page.route("**/api/consulting/deal-detail?*", (r) =>
    r.fulfill({ json: detailFixture }),
  );
  await page.route("**/api/crm/workspace/deals/*?*", (r) =>
    r.fulfill({
      json: {
        hubspot_portal_id: "123",
        selected: {
          deal: { actionyotei: "2026-10-21", teian: "架空の検討記録" },
        },
        partial: [],
      },
    }),
  );
  await page.route("**/api/crm/edit-schema?*", (r) =>
    r.fulfill({ json: { writes_enabled: false, editable: [] } }),
  );
  await page.route("**/api/job-copy/company-deals?*", (r) =>
    r.fulfill({
      json: {
        company: { id: "501", name: "架空の会社・別表記" },
        deals: [
          {
            id: "101",
            name: "架空の採用支援・秋",
            pipeline_id: "21596025",
            pipeline_label: "納品管理",
            stage_label: "支援中",
            active: true,
            current: true,
            created_at: "2026-08-01",
            contract_start: "2026-08-01",
            contract_end: "2026-12-31",
            listings_total: 1,
            listings_live: 1,
            recent_apps: 4,
            recent_cost: null,
          },
        ],
      },
    }),
  );
  await page.route("**/api/job-copy/listings?*", (r) =>
    r.fulfill({
      json: {
        status: "ready",
        total: 1,
        next_offset: null,
        titles: ["ドライバー"],
        offset: 0,
        index_built_at: "2026-10-18",
        refreshing: false,
        refresh_failed: false,
        listings: [{ ...fixtureJobs[1], deal_ids: ["101"] }],
      },
    }),
  );
  await page.route("**/api/job-copy/listings/2/versions", (r) =>
    r.fulfill({ json: fixtureHistory("2") }),
  );
  await page.route("**/api/job-copy/deal-applications?*", (r) =>
    r.fulfill({
      json: {
        listings: [
          {
            listing_id: "2",
            by_date: { "2026-10-16": 4 },
            fetched_at: "2026-10-18",
          },
        ],
        shared_applications: 0,
      },
    }),
  );
  await page.route("**/api/job-copy/body-dates?*", (r) =>
    r.fulfill({ json: { dates: { "2": "2026-10-01" } } }),
  );
  await login(page);
  await page.goto("/app/job-copy?view=houjin&owner=11&owner_field=consultant");
  const charts = page.locator('[data-chart-ready="true"]');
  await expect(page.getByRole("region", { name: "法人の数字" })).toContainText(
    "¥900,000",
  );
  await expect(page.getByTestId("hj-contracts")).toHaveAttribute("data-chart-ready", "true");
  expect(await charts.count()).toBe(1);
  await expect(page.getByRole("table", { name: "拠点の比較" })).toHaveCount(0);
  await expect(
    page.getByRole("link", { name: "他の法人と比べる（コンサルKPI）" }),
  ).toHaveAttribute("href", "/consulting#research/customer?houjin=sample-a");
  await page.getByText(/^分析の図と拠点の比較（図13点）/).click();
  await expect.poll(() => charts.count()).toBe(14);
  const rendered = await charts.evaluateAll((els) =>
    els.map((el) => {
      const instance = window.__echarts_getInstanceByDom?.(el as HTMLElement);
      const series = instance?.getOption().series as
        | { data?: unknown[] }[]
        | undefined;
      return (
        !!instance && !!series?.length && series.some((s) => s.data?.length)
      );
    }),
  );
  expect(rendered.every(Boolean)).toBe(true);
  expect(
    await page.getByTestId("hj-monthly-101").evaluate((el) => {
      const option = window.__echarts_getInstanceByDom?.(el)?.getOption();
      return (option?.series as { data: { value: number }[] }[]).map((s) =>
        s.data.map((p) => p.value),
      );
    }),
  ).toEqual([
    [4, 10, 20],
    [1, 3, 8],
    [1, 2, 5],
  ]);
  await expect(
    page.getByRole("heading", { name: "掲載中 1件", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("table", { name: "拠点の比較" })).toContainText(
    "6（2件）",
  );
  await mkdir(shots, { recursive: true });
  await page.screenshot({ path: `${shots}/01-houjin-numbers-sites.png` });
  await page
    .locator("#hj-site-details")
    .evaluate((el) =>
      window.scrollTo(0, el.getBoundingClientRect().top + scrollY - 125),
    );
  const mtg = page.getByRole("table", { name: "MTGの履歴" });
  for (const column of [1, 7]) {
    const date = mtg.locator(`tbody tr:first-child td:nth-child(${column})`);
    await expect(date).toHaveCSS("white-space", "nowrap");
    expect(await date.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
  }
  await page.screenshot({ path: `${shots}/02-houjin-trends-mtg.png` });
  await expect(page.getByRole("table", { name: "MTGの履歴" })).toContainText(
    "募集職種の追加",
  );
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.getByText("条件で絞る（ボタン・数値条件）").click();
  await page
    .getByRole("button", { name: "大きいのに拠点が少ない", exact: true })
    .click();
  await expect(page).toHaveURL(/houjin_sites_max=2/);
  await page.reload();
  await page.getByText("条件で絞る（ボタン・数値条件）").click();
  await expect(
    page.getByRole("button", { name: "大きいのに拠点が少ない", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await page
    .getByRole("table", { name: "契約の系列", exact: true })
    .getByRole("button", { name: "架空の採用支援・秋" })
    .click();
  await expect(page.getByRole("dialog")).toContainText("次の予定");
  await expect(
    page.getByRole("link", { name: "取引の画面を開く" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "閉じる", exact: true }).click();
  await page.getByRole("button", { name: "案件一覧", exact: true }).click();
  await page.getByRole("button", { name: "採用単価 ▲▼", exact: true }).click();
  await page.getByRole("button", { name: "採用単価 ▼", exact: true }).click();
  const table = page.locator(".db-board-scroll>.db-table");
  await expect(table.locator("tbody>tr").first()).toContainText(
    "架空の採用支援01",
  );
  await expect(
    page.getByRole("columnheader", { name: "採用単価 ▲", exact: true }),
  ).toHaveAttribute("aria-sort", "ascending");
  await expect(page.getByText("HubSpotの今の値と、シートにある最後のMTGを確認できます。")).toHaveCount(0);
  const frame = page.locator(".db-scroll-frame");
  await expect(frame).toHaveClass(/db-more-right/);
  await expect(frame).toHaveCSS("position", "relative");
  expect(await frame.evaluate(el => getComputedStyle(el, "::after").backgroundImage)).toContain("linear-gradient");
  await page.screenshot({ path: `${shots}/03-deals-cpa.png` });
  const scroll = page.getByRole("region", { name: "案件一覧（横スクロール）" });
  await scroll.evaluate(el => { el.scrollLeft = el.scrollWidth; });
  await expect(frame).not.toHaveClass(/db-more-right/);
  await scroll.evaluate(el => { el.scrollLeft = 0; });
  await expect(frame).toHaveClass(/db-more-right/);
  await table
    .getByRole("button", { name: "架空の採用支援01", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByRole("button", { name: "閉じる", exact: true }).click();
  await page
    .getByRole("combobox", { name: "担当者", exact: true })
    .selectOption("22");
  await expect(table.locator("tbody>tr")).toHaveCount(1);
  await expect(page.getByRole("combobox", { name: "担当者", exact: true })).toHaveValue("22");
  await expect(table).toContainText("架空の採用支援02");
  for (const label of ["危ない案件", "いい案件", "法人"]) {
    await page.getByRole("button", { name: label, exact: true }).click();
    await expect(
      page.getByRole("combobox", { name: "担当者", exact: true }),
    ).toHaveValue("22");
    await expect(page).toHaveURL(/owner=22/);
    if (label === "法人") {
      await expect(page.getByText("条件に合う法人がありません")).toBeVisible();
      await expect(page.getByRole("region", { name: "法人の数字" })).toHaveCount(0);
      await expect(page.getByRole("table", { name: "MTGの履歴" })).toHaveCount(0);
    } else
      await expect(
        page.getByText("該当する案件はありません。").first(),
      ).toBeVisible();
  }
  await page.screenshot({ path: `${shots}/04-member-houjin.png` });
  await page.reload();
  await expect(
    page.getByRole("combobox", { name: "担当者", exact: true }),
  ).toHaveValue("22");
  await expect(page.getByText("条件に合う法人がありません")).toBeVisible();
  await page.getByRole("button", { name: "条件を戻す", exact: true }).click();
  await expect(page.getByRole("region", { name: "法人の数字" })).toContainText("架空の法人わかば");
  await page.goto("/app/job-copy?view=houjin&owner=11&owner_field=consultant&houjin=sample-a&houjin_site=east");
  await page.getByRole("textbox", { name: "法人の検索" }).fill("わかば");
  await expect(page.getByRole("region", { name: "法人の数字" })).toContainText("架空の法人わかば");
  await expect(page).toHaveURL(/houjin=sample-b/);
  expect(new URL(page.url()).searchParams.has("houjin_site")).toBe(false);
  await page.goto("/app/job-copy?view=houjin&owner=11&owner_field=consultant");
  const listing = page
    .getByRole("table", { name: "掲載中の求人" })
    .getByRole("button", { name: "配送ドライバー・沖縄2", exact: true });
  await listing.click();
  await expect(page).toHaveURL(/listing=2/);
  await expect(page).toHaveURL(/deal=101/);
  expect(new URL(page.url()).searchParams.get("listing")).toBe("2");
  await expect(page.getByRole("heading", { name: "配送ドライバー・沖縄2", exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});
