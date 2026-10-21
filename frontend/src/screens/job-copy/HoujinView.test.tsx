// @vitest-environment happy-dom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { HoujinView } from "./HoujinView";
import {
  houjinFixture,
  houjinIndex,
  houjinData,
  memberHoujinFixture,
} from "./houjin.testFixtures";
import {
  filterCustomers,
  presetQuery,
  customerPresets,
  hireAt,
  monthAdd,
  sumKnown,
} from "./houjinModel";
import { boardFixture, boardRows } from "./dealBoard.testFixtures";
import { sortDeals } from "./dealBoardModel";
import { DealBoard } from "./DealBoard";
import { detailFixture } from "./consultantRisk.testFixtures";
vi.mock("../../components/EChart", () => ({
  EChart: ({ testId, option }: { testId: string; option: object }) => (
    <div data-testid={testId} data-option={JSON.stringify(option)} />
  ),
}));
vi.mock("./ProposalMemo", () => ({ ProposalMemo: () => <p>案件のメモ</p> }));
vi.mock("./NextTodo", () => ({
  NextTodoButton: () => <button>次の予定を記録</button>,
  ReasonChips: () => null,
}));
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
function required<T>(value: T | undefined): T {
  if (value === undefined) throw new Error("fixture missing");
  return value;
}
function mock() {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: string) => {
      const u = new URL(input, location.origin);
      const data = u.pathname.endsWith("/company-deals")
        ? { company: { id: "501", name: "架空の会社・別表記" }, deals: [] }
        : u.pathname.endsWith("/houjin")
          ? u.searchParams.get("owner") === "22" || u.searchParams.get("houjin") === "sample-b"
            ? { ...memberHoujinFixture, index: u.searchParams.get("owner") === "22" ? memberHoujinFixture.index : houjinIndex }
            : houjinFixture
          : u.pathname.endsWith("/deals")
            ? boardFixture
            : u.pathname.includes("deal-detail")
              ? detailFixture
              : u.pathname.endsWith("/listings")
                ? { status: "ready", total: 0, next_offset: null, listings: [] }
                : {
                    hubspot_portal_id: "123",
                    selected: { deal: { actionyotei: "2026-10-21" } },
                    partial: [],
                  };
      return Promise.resolve(
        new Response(JSON.stringify(data), {
          headers: { "content-type": "application/json" },
        }),
      );
    }),
  );
}
it("条件ボタン4つ・上下限・欠損とURLの往復", () => {
  for (const p of customerPresets) {
    const q = new URLSearchParams(
      Object.entries(presetQuery(p.values)).filter(
        (v): v is [string, string] => v[1] !== null,
      ),
    );
    expect(filterCustomers(houjinIndex, q).map((r) => r.houjin)).toEqual([
      "sample-a",
    ]);
    expect(
      filterCustomers(houjinIndex, new URLSearchParams(q.toString())),
    ).toEqual(filterCustomers(houjinIndex, q));
  }
  expect(
    filterCustomers(
      houjinIndex,
      new URLSearchParams("houjin_sites=2&houjin_sites_max=2"),
    ).map((r) => r.houjin),
  ).toEqual(["sample-a"]);
  expect(
    filterCustomers(
      houjinIndex,
      new URLSearchParams("houjin_sites=3&houjin_sites_max=2"),
    ),
  ).toEqual([]);
  expect(
    filterCustomers(
      [{ ...required(houjinIndex[0]), sites: null }],
      new URLSearchParams("houjin_sites_max=2"),
    ),
  ).toEqual([]);
  expect(sumKnown([null, null], (x) => x)).toBeNull();
  expect(sumKnown([0, null], (x) => x)).toBe(0);
  expect(monthAdd("2026-12-15", 2)).toBe("2027-01");
  expect(hireAt(required(houjinData.monthly[0]), "2027-01")).toBeNull();
  expect(hireAt(required(houjinData.monthly[0]), "2026-11")).toEqual({
    m: 3,
    v: 5,
    carry: true,
  });
});
it("法人の全節と数字・案件の詳細・条件のURLを表示", async () => {
  window.history.replaceState(null, "", "/app/job-copy?view=houjin&owner=11");
  mock();
  render(<HoujinView />);
  const stats = await screen.findByRole("region", { name: "法人の数字" });
  expect(stats.textContent).toContain("3件");
  expect(stats.textContent).toContain("2拠点");
  expect(stats.textContent).toContain("¥900,000");
  const sites = screen.getByRole("table", { name: "拠点の比較" });
  expect(within(sites).getByText("¥800,000")).toBeTruthy();
  expect(within(sites).getByText("6（2件）")).toBeTruthy();
  const mtg = screen.getByRole("table", { name: "MTGの履歴" });
  for (const t of [
    "低",
    "求人の見直し",
    "応募の偏り",
    "募集職種の追加",
    "2026-10-23",
  ])
    expect(within(mtg).getByText(t)).toBeTruthy();
  expect(screen.getByTestId("hj-monthly-101").dataset.option).toContain("20");
  expect(screen.getByTestId("hj-nps-101").dataset.option).toContain(
    "emptyCircle",
  );
  for (const title of ["こちらの動き", "取引と求人", "重点顧客の内訳"])
    expect(screen.getByRole("heading", { name: title })).toBeTruthy();
  const cpa = required(
    screen.getAllByRole("table", { name: "採用単価の3つの出し方" })[0],
  );
  expect(within(cpa).getByText("¥72,000")).toBeTruthy();
  expect(within(cpa).getByText("¥150,000")).toBeTruthy();
  const option = (id: string) => JSON.parse(screen.getByTestId(id).dataset.option ?? '{}') as {series: {data: unknown[]}[]};
  expect(option('hj-funnel').series.map(s=>s.data)).toEqual([[24,9,6]]);
  expect(option('hj-ltv').series.map(s=>s.data)).toEqual([[200000,800000],[100000,100000]]);
  expect(option('hj-hires').series[0]?.data).toEqual([{value:1,symbol:'circle',symbolSize:5},{value:2,symbol:'circle',symbolSize:5},{value:5,symbol:'circle',symbolSize:5}]);
  expect(screen.getByRole('table',{name:'接触と担当交代'}).textContent).toContain('2026-09-01 架空の担当A → 架空の担当B（あり）');
  fireEvent.click(screen.getByRole('checkbox',{name:/架空の採用支援・秋/}));
  expect(option('hj-funnel').series.map(s=>s.data)).toEqual([[4,1,1]]);
  expect(screen.getByTestId('hj-monthly-101').dataset.option).toContain('20');
  fireEvent.click(
    screen.getByRole("button", { name: "大きいのに拠点が少ない" }),
  );
  expect(location.search).toContain("houjin_sites_max=2");
  fireEvent.click(
    within(screen.getByRole("table", { name: "契約の系列" })).getByRole(
      "button",
      { name: "架空の採用支援・秋" },
    ),
  );
  expect(await screen.findByRole("dialog")).toBeTruthy();
  expect(
    await screen.findByRole("link", { name: "取引の画面を開く" }),
  ).toBeTruthy();
});
it("応募が1件も無い取引だけを、求人票の数とHubSpotへのリンク付きで出す", async () => {
  window.history.replaceState(
    null,
    "",
    "/app/job-copy?view=deals&owner=11&deals_zero=yes",
  );
  mock();
  render(<DealBoard />);
  expect(
    (await screen.findByRole("heading", { name: /応募が1件も無い取引/ }))
      .textContent,
  ).toBe("応募が1件も無い取引 1件");
  expect(screen.getByRole("button", { name: "架空の採用支援01" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "架空の採用支援02" })).toBeNull();
  expect(screen.getByText("3件")).toBeTruthy();
  expect(
    screen.getByRole("link", { name: "HubSpotで開く" }).getAttribute("href"),
  ).toBe("https://app.hubspot.com/contacts/1234567/record/0-3/100");
});
it("案件一覧の新しい列を両方向に並べ替える", async () => {
  window.history.replaceState(null, "", "/app/job-copy?view=deals&owner=11");
  mock();
  render(<DealBoard />);
  await screen.findByRole("button", { name: "架空の採用支援01" });
  for (const [key, label] of [
    ["cpa", "採用単価"],
    ["nps", "NPS"],
    ["recent_applications", "応募（直近4週）"],
    ["hires", "採用"],
    ["amount", "金額"],
  ] as const) {
    const button = screen.getByRole("button", { name: `${label} ▲▼` });
    fireEvent.click(button);
    await waitFor(() => {
      expect(button.closest("th")?.getAttribute("aria-sort")).toBe(
        "descending",
      );
    });
    fireEvent.click(button);
    await waitFor(() => {
      expect(button.closest("th")?.getAttribute("aria-sort")).toBe("ascending");
    });
    const rows = [
      { ...required(boardRows[0]), [key]: 100 },
      { ...required(boardRows[1]), [key]: 0 },
      { ...required(boardRows[2]), [key]: null },
    ];
    expect(sortDeals(rows, key, true).map((r) => r.id)).toEqual([
      "100",
      "101",
      "102",
    ]);
    expect(sortDeals(rows, key, false).map((r) => r.id)).toEqual([
      "101",
      "100",
      "102",
    ]);
  }
});

it("条件が0社なら法人の内容を隠し、解除すると復元する", async () => {
  window.history.replaceState(null, "", "/app/job-copy?view=houjin&owner=11");
  mock();
  render(<HoujinView />);
  await screen.findByRole("region", { name: "法人の数字" });
  fireEvent.change(screen.getByRole("textbox", { name: "法人の検索" }), { target: { value: "該当なし" } });
  expect(screen.getByText("条件に合う法人がありません")).toBeTruthy();
  expect(screen.queryByRole("region", { name: "法人の数字" })).toBeNull();
  expect(screen.queryByRole("table", { name: "MTGの履歴" })).toBeNull();
  expect(screen.queryByRole("heading", { name: "取引と求人" })).toBeNull();
  expect(screen.getByRole<HTMLSelectElement>("combobox", { name: "法人を選ぶ" }).value).toBe("");
  fireEvent.click(screen.getByRole("button", { name: "条件を戻す" }));
  expect(await screen.findByRole("region", { name: "法人の数字" })).toBeTruthy();
});
it("選択中の法人が条件から外れると先頭へ切り替え、拠点と詳細を解除する", async () => {
  window.history.replaceState(null, "", "/app/job-copy?view=houjin&owner=11&houjin_site=east&risk_deal=101");
  mock();
  render(<HoujinView />);
  await screen.findByRole("region", { name: "法人の数字" });
  fireEvent.change(screen.getByRole("textbox", { name: "法人の検索" }), { target: { value: "わかば" } });
  await waitFor(() => {
    expect(screen.getByRole("region", { name: "法人の数字" }).textContent).toContain("架空の法人わかば");
  });
  expect(location.search).toContain("houjin=sample-b");
  expect(location.search).not.toContain("houjin_site");
  expect(location.search).not.toContain("risk_deal");
  expect(screen.getByRole<HTMLSelectElement>("combobox", { name: "法人を選ぶ" }).options.length).toBe(2);
});
it("担当変更後も絞り込みを保ち、0社なら法人を出さない", async () => {
  window.history.replaceState(null, "", "/app/job-copy?view=houjin&owner=22&houjin_employees=1000");
  mock();
  render(<HoujinView />);
  await screen.findByText("条件に合う法人がありません");
  expect(screen.queryByRole("region", { name: "法人の数字" })).toBeNull();
});
