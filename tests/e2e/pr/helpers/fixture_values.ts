/**
 * scripts/e2e/make_fixture_db.py の postings fixture の既知値。
 * fixture の行を足したら、ここも更新する (smoke.spec.ts が /health の db_rows で照合する)。
 */
export const FIXTURE = {
  postingsTotal: 63,
  // 都道府県 → 市区町村 (JIS 北→南順で /api/prefectures は東京都、大阪府の順)
  prefectures: ['東京都', '大阪府'],
  municipalities: {
    東京都: ['千代田区', '新宿区', '港区'], // /api/municipalities_cascade は ORDER BY municipality (文字コード順)
    大阪府: ['堺市', '大阪市'],
  },
  // 千代田区: 正社員 10 / パート 5
  chiyoda: { fulltime: 10, parttime: 5, total: 15 },
} as const;

export const E2E_EMAIL = 'e2e@f-a-c.co.jp';
export const E2E_PASSWORD = 'testpass';
export const PR_PORT = 9217;
export const PR_BASE_URL = `http://localhost:${PR_PORT}`;

/**
 * 採用診断 (recruitment_diag) の既知値。scripts/e2e/make_fixture_db.py の docstring に計算過程がある。
 * 条件: 業種 飲食業 / 雇用形態 正社員 / 東京都 (prefcode 13) / 千代田区 (citycode 13101)。
 * 数値は Rust の式を手計算し、同じ SQL を python sqlite3 で流して一致を確認した値。
 * 示唆 (insights) の文言は src/handlers/recruitment_diag/testdata/snapshots/insights__rich_city.json
 * (cargo test が検証している実出力) と同じ。python では確認できていない (Rust の insight エンジンが生成する)。
 * 画面に出る文字列は、旧画面 (templates/tabs/recruitment_diag.html) の書式で数値だけ取り出して比べる。
 */
export const RD_FIXTURE = {
  /** 東京都全体 (市区町村なし) の 飲食業 / 正社員 の HW 件数: 千代田区 5 + 港区 4 + 新宿区 3 (各市区町村の正社員のうち k が奇数の行)。 */
  prefWideHwCount: 12,
  jobType: '飲食業',
  empType: '正社員',
  pref: '東京都',
  prefcode: 13,
  city: '千代田区',
  citycode: 13101,
  // 自社条件 (画面入力は月給が万円単位)。年収 = 280000 x (12 + 2.5) = 4,060,000
  own: { salaryMan: 28, salaryYen: 280000, holidays: 125, bonus: 2.5 },

  // Panel 1: 5 / 昼 32000 (= (22000+10000) x 2ヶ月 / 2) / 夜 25000 / score = 5/32000 x 10000 = 1.5625 / 全国 20 / 5/20 = 25%
  difficulty: {
    hwCount: 5,
    nationalHwCount: 20,
    dayPopulation: 32000,
    nightPopulation: 25000,
    dayNightRatio: 1.28,
    scorePer10k: 1.5625,
    scoreDisplay: '1.6', // 旧画面は小数 1 桁
    rank: 2,
    rankLabel: '穏やか',
    areaSharePctDisplay: '25.00', // 旧画面は % を小数 2 桁
    soWhat:
      '1万人あたり 1.6 件。競合は存在するが採用競争は過熱していない傾向。差別化条件（賞与・年休）を明確にすれば応募は獲得しやすい可能性。',
  },
  // Panel 2: 昼 32000 / 夜 25000 / 流入 +7000 / 昼夜比 1.28
  talentPool: { day: 32000, night: 25000, inflow: 7000, ratio: 1.28 },
  // Panel 3: 画面は「開発中」。API の流入元内訳 (year=2021, 平日昼の全月 SUM): from_area 0..3 = 6000 / 4000 / 3000 / 3000
  inflow: { populations: [6000, 4000, 3000, 3000], total: 16000, shares: [0.375, 0.25, 0.1875, 0.1875] },
  // Panel 4 / 6: Turso・SalesNow が無い環境では必ずエラー (src/handlers/recruitment_diag/testdata/snapshots/*no_*.json)
  competitorsError: 'SalesNow DB 未接続',
  marketTrendError: 'Turso DB 未接続',
  // Panel 5: 業界 (千代田区 正社員 飲食業 5 行) 月給中央値 250000 (昇順 index 2) x (12+2.0) = 3,500,000 / n=5
  //          全業界 (千代田区 正社員 10 行) 月給中央値 250000 (昇順 index 5) x 14 = 3,500,000 / n=10
  //          どちらも 年休 120 / 賞与 2.0。自社 4,060,000 との差 +560,000 (+16.0%)、年休 +5、賞与 +0.5
  conditionGap: {
    industry: { annualIncome: 3500000, annualHolidays: 120, bonusMonths: 2.0, sampleSize: 5 },
    allIndustry: { annualIncome: 3500000, annualHolidays: 120, bonusMonths: 2.0, sampleSize: 10 },
    ownAnnualIncome: 4060000,
    // 旧画面の表示 (年収は万円、差は符号つき)
    display: {
      industry: { n: '5', annual: '350', holidays: '120', bonus: '2.0' },
      allIndustry: { n: '10', annual: '350', holidays: '120', bonus: '2.0' },
      gaps: ['+56万', '+5日', '+0.5ヶ月'],
    },
    interpretation:
      '【東京都・飲食業】御社推定年収は業界中央値より 560000円 (16.0%) 上回る傾向。年間休日は業界中央値より 5日多い傾向。サンプル数 5件。※中央値は HW 掲載求人のみから算出。市場全体の実勢ではない。',
  },
  // 自社条件を 0 で入力したとき (入力値 0)。年収 0 x (12 + 0) = 0 → 中央値 3,500,000 との差 -3,500,000 (-100.0%)、
  // 年休 0 - 120 = -120、賞与 0 - 2.0 = -2.0。業界・全業界とも同じ中央値なので同じ差。
  conditionGapZero: {
    gaps: ['-350万', '-120日', '-2.0ヶ月'],
    interpretation:
      '【東京都・飲食業】御社推定年収は業界中央値より 3500000円 (100.0%) 下回る傾向。年間休日は業界中央値より 120日少ない傾向。サンプル数 5件。※中央値は HW 掲載求人のみから算出。市場全体の実勢ではない。',
  },
  // Panel 1 / Panel 7 共通の注記 (しきい値は handlers.rs classify_difficulty と opportunity_map.rs の定数から転記)
  thresholdNote:
    '※ 区分の基準はパネルごとに異なります。Panel 1: 分母の人口1万人あたり求人数で 1 未満 穴場 / 3 未満 穏やか / 7 未満 平均的 / 15 未満 激戦 / 15 以上 超激戦。Panel 7: 昼間人口1万人あたり求人数で 5 未満 穴場 / 20 未満 標準 / 20 以上 激戦。',
  // Panel 7: 東京都 飲食業 正社員 (人口 1 万人あたり)。千代田区 5件/昼間人口 4000 x 10000 = 12.5 標準 (5 以上 20 未満)、
  //          港区 4/1600 x 10000 = 25 激戦 (20 以上)、新宿区 3/12000 x 10000 = 2.5 穴場 (5 未満)
  opportunity: {
    // API の並び (スコア降順)
    municipalities: [
      { name: '港区', hwCount: 4, population: 1600, score: 25, category: '激戦' },
      { name: '千代田区', hwCount: 5, population: 4000, score: 12.5, category: '標準' },
      { name: '新宿区', hwCount: 3, population: 12000, score: 2.5, category: '穴場' },
    ],
    count: 3,
  },
  // Panel 8: HS-3 (重大) → HS-1 (注意) → AP-2 (情報) の順
  insights: [
    {
      id: 'HS-3',
      title: '求人情報の開示不足',
      message:
        '求人情報の開示度は30%と低く、特に「残業時間」の開示率が10%にとどまっています。情報量が少ない求人は応募率が低下する傾向があります。',
      action:
        '求人票の情報開示が不足している傾向。勤務時間・休日・福利厚生の具体記載で応募数改善の余地があります。',
    },
    {
      id: 'HS-1',
      title: '慢性的人材不足シグナル',
      message:
        '正社員の欠員補充率（求人理由が「欠員補充」の比率）は25.0%の水準にあり、人材確保に困難が生じる可能性があります。時系列データなし',
      action:
        '慢性的な人材不足の可能性あり。給与水準の再設計や採用広告の露出拡大、入社後定着施策の強化を検討する価値があります。',
    },
    {
      id: 'AP-2',
      title: '求人原稿の改善提案',
      message:
        '以下の情報を追加開示してください: 残業時間、女性比率。情報量が多い求人は応募率が高まる傾向があります。',
      action: '本示唆は既にアクション提案形式です。現場の実情と照らして実行可否を判断してください。',
    },
  ],
  // Panel 9: 千代田区 宛て OD の上位 12 (荒川区 5000 は 13 番目で除外、自市区町村 99999 と別宛先 55555 も除外)
  //   30 分圏 = 上位 5: 失業者 2500+1800+1600+2200+900 = 9000 / HW 新宿区 10 のみ = 10
  //   60 分圏 = 次の 7: 失業者 4100+3000+3500+1700+5200+2300+2600 = 22400 / HW 港区 12 のみ = 12
  expansion: {
    tier30: { count: 5, unemploymentPool: 9000, hwPostings: 10 },
    tier60: { count: 7, unemploymentPool: 22400, hwPostings: 12 },
    // [都道府県 市区町村, 通勤者数, 失業者数, HW 求人]。30 分圏 → 60 分圏の順
    rows: [
      ['東京都 新宿区', 90000, 2500, 10],
      ['東京都 文京区', 80000, 1800, 0],
      ['東京都 台東区', 70000, 1600, 0],
      ['東京都 渋谷区', 60000, 2200, 0],
      ['東京都 中央区', 50000, 900, 0],
      ['東京都 江東区', 40000, 4100, 0],
      ['東京都 港区', 35000, 3000, 12],
      ['東京都 品川区', 30000, 3500, 0],
      ['東京都 目黒区', 25000, 1700, 0],
      ['東京都 世田谷区', 20000, 5200, 0],
      ['東京都 豊島区', 15000, 2300, 0],
      ['東京都 北区', 10000, 2600, 0],
    ],
    statusText: '完了（12 市区町村）',
  },
} as const;

/**
 * 営業KPI (tests/fixtures/sales_kpi、判定日 2026-09-04) の既知値。
 * `cargo run --example dump_sales_kpi` の JSON の by_person / by_team / calls.periods.this_week を Python で足した値
 * (Rust の契約テスト src/handlers/sales_kpi/tests.rs と同じ: 伊壺チーム apo 31 / pool 66 / 実施 19、stale 9)。
 */
export const SALES_KPI_FIXTURE = {
  today: '2026-09-04',
  all: { apo: 245, pool: 537, done: 163, den: 225, cyomi: 123 }, // den = 実施 163 + 未実施 53 + 未処理 4 + 要判定 5
  iduboTeam: { apo: 31, pool: 66, done: 19 }, // ④ = 19 + 10 + 1 = 30
  stale: 9,
  // 今週 (8/31〜9/4 の 5 日) の全社。by_person の合計 (突合できた分)
  calls: { connected: 29866, calls: 34295, long: 791 },
} as const;

/**
 * 競合調査 (/competitor と /app/competitor) の既知値。tests/fixtures/competitor/sp_utf8.csv (架空の合成 CSV) から
 * scripts/e2e/competitor_expected.py が Rust と無関係に計算した値 (python scripts/e2e/competitor_expected.py)。
 * 給与は表示の書式 (月給 = 万円 小数 2 桁 / 時給 = 円の整数)。上位 N 件の N は 10。
 * 画面に出る文字列は旧画面 (/report/competitor の HTML) の書式。
 */
export const COMPETITOR_FIXTURE = {
  "total": 60,
  "monthly": {
    "counts": [
      44,
      44,
      14,
      14
    ],
    "all_lower": [
      "26.98",
      "27.00",
      "20.00"
    ],
    "all_upper": [
      "31.93",
      "32.00",
      "23.00"
    ],
    "pop_lower": [
      "26.93",
      "26.50",
      "20.00"
    ],
    "pop_upper": [
      "31.79",
      "31.50",
      "23.00"
    ],
    "diff_lower": [
      "0.05",
      "0.50",
      "0.00"
    ],
    "diff_upper": [
      "0.15",
      "0.50",
      "0.00"
    ],
    "hist_upper": [
      [
        "20",
        1
      ],
      [
        "21",
        2
      ],
      [
        "22",
        2
      ],
      [
        "23",
        9
      ],
      [
        "24",
        1
      ],
      [
        "25",
        3
      ],
      [
        "26",
        4
      ],
      [
        "27",
        3
      ],
      [
        "28",
        3
      ],
      [
        "29",
        3
      ],
      [
        "30",
        3
      ],
      [
        "31",
        3
      ],
      [
        "32",
        3
      ],
      [
        "33",
        3
      ],
      [
        "34",
        3
      ],
      [
        "35",
        2
      ],
      [
        "36",
        3
      ],
      [
        "37",
        3
      ],
      [
        "39",
        3
      ],
      [
        "40",
        3
      ]
    ],
    "hist_lower": [
      [
        "19",
        4
      ],
      [
        "20",
        7
      ],
      [
        "21",
        7
      ],
      [
        "22",
        5
      ],
      [
        "23",
        5
      ],
      [
        "24",
        3
      ],
      [
        "25",
        3
      ],
      [
        "26",
        3
      ],
      [
        "27",
        3
      ],
      [
        "28",
        2
      ],
      [
        "29",
        3
      ],
      [
        "30",
        3
      ],
      [
        "31",
        3
      ],
      [
        "32",
        3
      ],
      [
        "33",
        3
      ],
      [
        "34",
        3
      ]
    ]
  },
  "hourly": {
    "counts": [
      16,
      16,
      4,
      4
    ],
    "all_lower": [
      "1263",
      "1255",
      "1150"
    ],
    "all_upper": [
      "1409",
      "1405",
      "1250"
    ],
    "pop_lower": [
      "1225",
      "1225",
      "1150"
    ],
    "pop_upper": [
      "1363",
      "1350",
      "1250"
    ],
    "diff_lower": [
      "38",
      "30",
      "0"
    ],
    "diff_upper": [
      "46",
      "55",
      "0"
    ],
    "hist_upper": [
      [
        "1250",
        1
      ],
      [
        "1300",
        4
      ],
      [
        "1350",
        2
      ],
      [
        "1400",
        4
      ],
      [
        "1450",
        1
      ],
      [
        "1500",
        3
      ],
      [
        "1550",
        1
      ]
    ],
    "hist_lower": [
      [
        "1150",
        4
      ],
      [
        "1200",
        4
      ],
      [
        "1250",
        2
      ],
      [
        "1300",
        3
      ],
      [
        "1350",
        2
      ],
      [
        "1400",
        1
      ]
    ]
  },
  "keywords_all": [
    [
      "交通費支給",
      18,
      60,
      "30"
    ],
    [
      "昇給あり",
      18,
      60,
      "30"
    ],
    [
      "社会保険完備",
      18,
      60,
      "30"
    ],
    [
      "未経験歓迎",
      17,
      60,
      "28"
    ],
    [
      "賞与あり",
      17,
      60,
      "28"
    ],
    [
      "駅チカ",
      16,
      60,
      "27"
    ],
    [
      "人気",
      14,
      60,
      "23"
    ],
    [
      "超人気",
      4,
      60,
      "7"
    ]
  ],
  "keywords_head": [
    [
      "交通費支給",
      4,
      10,
      "40"
    ],
    [
      "社会保険完備",
      4,
      10,
      "40"
    ],
    [
      "昇給あり",
      3,
      10,
      "30"
    ],
    [
      "未経験歓迎",
      3,
      10,
      "30"
    ],
    [
      "賞与あり",
      3,
      10,
      "30"
    ],
    [
      "駅チカ",
      3,
      10,
      "30"
    ],
    [
      "人気",
      2,
      10,
      "20"
    ],
    [
      "超人気",
      1,
      10,
      "10"
    ]
  ]
} as const;
