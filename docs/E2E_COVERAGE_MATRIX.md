# E2E Coverage Matrix

**作成日**: 2026-04-12
**目的**: E2E_TEST_PLAN.md (78ケース) と E2E_TEST_PLAN_V2.md (74ケース) に対する、本セッション実施E2Eスクリプトのカバレッジマッピング

---

## 本セッション実施済みE2Eスクリプト一覧

| スクリプト | 主テスト内容 | 項目数 |
|-----------|-------------|--------|
| `e2e_report_survey.py` | /report/survey CSVアップロード、ECharts、ソート | ~10 |
| `e2e_report_jobbox.py` | 求人ボックス形式CSV処理 | ~5 |
| `e2e_report_insight.py` | /report/insight 品質検証（22パターン等） | ~10 |
| `e2e_other_tabs.py` | 市場概況/地図/詳細分析/企業検索/条件診断 | 30 |
| `e2e_api_excel.py` | JSON API / xlsx出力 / upload POST | 36 |
| `e2e_security.py` | XSS(7) / 大容量 / 文字コード / SQLi / CSRF / ファイル偽装 | 29 |
| `e2e_print_verify.py` | PDF生成 (pypdf検証) | ~6 |

**凡例（カバレッジ率）**:
- ✅ 完全 = 該当テストの検証内容をスクリプトが網羅
- 🟡 部分 = 一部項目のみ、追加検証が必要
- ❌ 未実施 = カバーされていない

---

## 1. E2E_TEST_PLAN.md (78ケース) マッピング

### 認証・セッション (AUTH) / 基盤 (INFRA)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| AUTH-001 ログイン成功 | P0 | e2e_other_tabs, e2e_api_excel (ログイン前提) | 🟡 部分 | Set-Cookie明示検証なし |
| AUTH-002 不正パスワード拒否 | P0 | e2e_security (レート制限系の前段で試行) | 🟡 部分 | 専用の302確認テストなし |
| AUTH-003 未認証API保護 | P0 | e2e_api_excel (schema stability一部) | 🟡 部分 | 全保護ルートの302確認なし |
| AUTH-004 セッション持続性 | P1 | e2e_other_tabs (5タブ連続遷移) | ✅ 完全 | - |
| AUTH-005 レート制限 | P2 | ❌ | ❌ 未実施 | 10回連続不正試行→429 |
| INFRA-001 /health | P0 | e2e_api_excel (JSON API) | 🟡 部分 | db_connected:true明示確認 |
| INFRA-002 静的ファイル配信 | P0 | ❌ | ❌ 未実施 | /static/js/*.js 200確認 |
| INFRA-003 GeoJSON配信 | P1 | e2e_other_tabs (地図タブ) | 🟡 部分 | 47都道府県全配信の網羅なし |

### 概要タブ (OV)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| OV-001 総求人件数の正確性 | P0 | e2e_other_tabs (市場概況KPI) | ✅ 完全 | - |
| OV-002 都道府県カスケード | P0 | e2e_other_tabs (条件診断/企業検索) | 🟡 部分 | 50以上選択肢の明示確認 |
| OV-003 フィルタ適用後の変化 | P0 | e2e_other_tabs | ✅ 完全 | - |
| OV-004 産業ツリー | P1 | ❌ | ❌ 未実施 | 大分類10種以上確認 |
| OV-005 KPI数学整合性 | P1 | e2e_other_tabs (KPI抽出) | 🟡 部分 | 施設数<求人数比較なし |
| OV-006 雇用形態分布合計 | P1 | ❌ | ❌ 未実施 | EChartsのpie合計=総件数 |

### 企業分析 (BAL) / 求人条件 (WS) / 採用動向 (DEMO)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| BAL-001〜004 | P1/P2 | e2e_other_tabs (部分的) | 🟡 部分 | 従業員規模・資本金・設立年代の妥当性検証 |
| WS-001〜004 | P1/P2 | ❌ | ❌ 未実施 | 雇用形態合計/給与範囲/福利厚生率 |
| DEMO-001〜003 | P1/P2 | ❌ | ❌ 未実施 | 求人理由/年齢制限/学歴要件分布 |

### 求人地図 (MAP)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| MAP-001 地図初期表示 | P1 | e2e_other_tabs (Leaflet確認) | ✅ 完全 | - |
| MAP-002 マーカーAPI | P1 | e2e_other_tabs | 🟡 部分 | prefectureパラメータ指定検証 |
| MAP-003 座標範囲妥当性 | P1 | ❌ | ❌ 未実施 | lat/lon日本範囲チェック |
| MAP-004 企業マーカーAPI | P2 | ❌ | ❌ 未実施 | SalesNow企業マーカー |
| MAP-005 コロプレスAPI | P2 | ❌ | ❌ 未実施 | /api/jobmap/choropleth |
| MAP-006 求人詳細 | P1 | ❌ | ❌ 未実施 | /api/jobmap/detail/{id} |
| MAP-007 地域統計サイドバー | P2 | ❌ | ❌ 未実施 | /api/jobmap/region/summary |

### 市場分析サブタブ (ANA)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| ANA-001 6サブタブ表示 | P0 | e2e_other_tabs (詳細分析タブ) | 🟡 部分 | subtab/1-6の各200確認なし |
| ANA-002 欠員率0-100% | P1 | ❌ | ❌ 未実施 | v2_vacancy_rate範囲 |
| ANA-003 透明性スコア | P2 | ❌ | ❌ 未実施 | 0-8範囲 |
| ANA-004 給与P25<P50<P75<P90 | P0 | ❌ | ❌ 未実施 | パーセンタイル昇順 |
| ANA-005 給与現実性 | P1 | ❌ | ❌ 未実施 | P25>=130,000円 |
| ANA-006 キーワード6カテゴリ | P2 | ❌ | ❌ 未実施 | 急募/未経験/待遇/WLB/成長/安定 |
| ANA-007 テキスト温度計 | P2 | ❌ | ❌ 未実施 | NaN/null検出 |
| ANA-008 4象限戦略=100% | P1 | ❌ | ❌ 未実施 | 合計100±1% |
| ANA-009 HHI指数 | P2 | ❌ | ❌ 未実施 | 0-10000範囲 |
| ANA-010 最低賃金違反率 | P1 | ❌ | ❌ 未実施 | 0-20% |
| ANA-011 地域ベンチマーク6軸 | P2 | ❌ | ❌ 未実施 | レーダー0-100 |
| ANA-012 充足スコアグレード | P2 | ❌ | ❌ 未実施 | A/B/C/D分布 |
| ANA-013 サブタブ間整合性 | P1 | ❌ | ❌ 未実施 | 欠員率↔充足スコア |
| ANA-014 フィルタ変更更新 | P1 | ❌ | ❌ 未実施 | 6サブタブ全更新 |

### 市場診断 (DIAG)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| DIAG-001 フォーム表示 | P1 | e2e_other_tabs (条件診断) | ✅ 完全 | - |
| DIAG-002 診断標準条件 | P0 | e2e_other_tabs | ✅ 完全 | - |
| DIAG-003 パーセンタイル妥当性 | P1 | e2e_other_tabs | 🟡 部分 | 40-70範囲の数値検証 |
| DIAG-004 高条件S/A | P1 | ❌ | ❌ 未実施 | 月給50万グレード |
| DIAG-005 低条件C/D | P1 | ❌ | ❌ 未実施 | 月給16万グレード |
| DIAG-006 月給0バリデーション | P1 | e2e_security (境界値) | 🟡 部分 | 専用メッセージ確認 |
| DIAG-007 負値バリデーション | P2 | ❌ | ❌ 未実施 | salary=-100000 |
| DIAG-008 リセット機能 | P2 | ❌ | ❌ 未実施 | /api/diagnostic/reset |
| DIAG-009 EChartsレーダー | P1 | e2e_other_tabs (data-chart-config) | ✅ 完全 | - |

### 詳細検索 (COMP) / 企業 (CO)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| COMP-001〜004 | P1/P2 | e2e_other_tabs (詳細分析) | 🟡 部分 | ページネーション/給与統計整合性 |
| CO-001 2文字制限 | P1 | e2e_api_excel (境界) | ✅ 完全 | - |
| CO-002 正常検索 | P0 | e2e_other_tabs (企業検索) | ✅ 完全 | - |
| CO-003 プロフィール表示 | P1 | e2e_other_tabs | 🟡 部分 | 市場分析情報含有 |
| CO-004 市場コンテキスト | P1 | ❌ | ❌ 未実施 | 平均給与範囲検証 |
| CO-005 近隣企業 | P2 | ❌ | ❌ 未実施 | 近隣セクション |

### API v1 (API)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| API-001 企業検索正常系 | P0 | e2e_api_excel (JSON schema) | ✅ 完全 | - |
| API-002 2文字制限 | P1 | e2e_api_excel | ✅ 完全 | - |
| API-003 空クエリ | P1 | e2e_api_excel | ✅ 完全 | - |
| API-004 プロフィールJSON | P1 | e2e_api_excel (schema) | ✅ 完全 | - |
| API-005 給与min<=max | P1 | e2e_api_excel (integrity) | 🟡 部分 | 明示的差異検証 |
| API-006 近隣API構造 | P2 | e2e_api_excel | ✅ 完全 | - |
| API-007 求人一覧API | P2 | e2e_api_excel | ✅ 完全 | - |
| API-008 存在しない企業番号 | P1 | e2e_api_excel (error handling) | ✅ 完全 | - |
| API-009 SQLi耐性 | P0 | e2e_security | ✅ 完全 | - |
| API-010 XSS耐性 | P0 | e2e_security | ✅ 完全 | - |

### サーベイ (SV) / ガイド (GUIDE) / フィルタ (FILT)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| SV-001 アップロードフォーム | P1 | e2e_report_survey | ✅ 完全 | - |
| SV-002 空ファイル | P1 | e2e_security (ファイル形式) | ✅ 完全 | - |
| SV-003 不正フォーマット | P1 | e2e_security (ファイル偽装) | ✅ 完全 | - |
| GUIDE-001 表示 | P3 | ❌ | ❌ 未実施 | /tab/guide コンテンツ>200B |
| FILT-001 47都道府県 | P0 | ❌ | ❌ 未実施 | /api/prefectures 全47 |
| FILT-002 市区町村カスケード | P0 | e2e_other_tabs (部分) | 🟡 部分 | 55以上の明示確認 |
| FILT-003 産業ツリー構造 | P1 | ❌ | ❌ 未実施 | 2階層構造 |
| FILT-004 セッション永続化 | P1 | e2e_other_tabs (多タブ) | ✅ 完全 | - |

### キャッシュ / クロス整合性 / Charts / パフォーマンス / エラー

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| CACHE-001 キャッシュ高速化 | P2 | ❌ | ❌ 未実施 | 2回目 < 50%時間 |
| CACHE-002 フィルタ変更無効化 | P1 | ❌ | ❌ 未実施 | キャッシュキー再計算 |
| CROSS-001 4タブ総件数一致 | P0 | ❌ | ❌ 未実施 | 差異0件 |
| CROSS-002 東京フィルタ3タブ一致 | P1 | ❌ | ❌ 未実施 | - |
| CROSS-003 マーカー/検索件数 | P2 | ❌ | ❌ 未実施 | 10倍以内 |
| CROSS-004 API v1/HTMLタブ一致 | P1 | ❌ | ❌ 未実施 | - |
| CHART-001 JSON有効性 | P1 | e2e_report_insight (data-chart-config) | ✅ 完全 | - |
| CHART-002 series非空 | P1 | e2e_report_insight, survey | ✅ 完全 | - |
| CHART-003 pie合計100% | P2 | ❌ | ❌ 未実施 | - |
| PERF-001 初回30秒 | P1 | ❌ | ❌ 未実施 | DOMContentLoaded計測 |
| PERF-002 タブ切替10秒 | P2 | ❌ | ❌ 未実施 | - |
| PERF-003 API 5秒 | P2 | ❌ | ❌ 未実施 | - |
| ERR-001 DB未接続 | P2 | ❌ | ❌ 未実施 | - |
| ERR-002 存在しないルート | P2 | ❌ | ❌ 未実施 | 404確認 |
| ERR-003 巨大クエリ | P3 | e2e_security (大容量) | ✅ 完全 | - |
| ERR-004 同時リクエスト | P3 | ❌ | ❌ 未実施 | 5並行 |

---

## 2. E2E_TEST_PLAN_V2.md (74ケース) マッピング

### Suite 1: AUTH (7) / Suite 2: CHART (12)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| AUTH-01 ログインページ表示 | P0 | e2e_other_tabs (前提) | 🟡 部分 | input要素明示検証 |
| AUTH-02 正常ログイン | P0 | 全スクリプト前提 | ✅ 完全 | - |
| AUTH-03 不正パス拒否 | P0 | ❌ | ❌ 未実施 | エラーメッセージ表示 |
| AUTH-04 ドメイン外拒否 | P0 | e2e_security | 🟡 部分 | hacker@evil.com試行 |
| AUTH-05 未認証APIアクセス | P0 | ❌ | ❌ 未実施 | new_context使用 |
| AUTH-06 ログアウト後無効化 | P1 | ❌ | ❌ 未実施 | /logout→遷移確認 |
| AUTH-07 レート制限 | P1 | ❌ | ❌ 未実施 | 10回連続 |
| CHART-01 ドーナツにデータ | P0 | e2e_other_tabs (市場概況) | ✅ 完全 | - |
| CHART-02 給与帯棒グラフ | P0 | e2e_other_tabs | 🟡 部分 | 3カテゴリ以上確認 |
| CHART-03 産業別グループ棒 | P0 | ❌ | ❌ 未実施 | 2系列両方データ |
| CHART-04 ドーナツ合計↔KPI | P1 | ❌ | ❌ 未実施 | 10%以内一致 |
| CHART-05 canvas非空白ピクセル | P0 | ❌ | ❌ 未実施 | 非透明率>1% |
| CHART-06 JSONパースエラー | P0 | ❌ | ❌ 未実施 | consoleエラー0 |
| CHART-07 タブ別チャート数 | P1 | e2e_other_tabs (部分) | 🟡 部分 | 各タブmin_charts明示 |
| CHART-08 リサイズ追従 | P2 | ❌ | ❌ 未実施 | viewport変更 |
| CHART-09 フィルタ→データ更新 | P1 | ❌ | ❌ 未実施 | before!=after |
| CHART-10 スタック100%以下 | P2 | ❌ | ❌ 未実施 | 雇用形態スタック |
| CHART-11 Top15降順 | P2 | ❌ | ❌ 未実施 | isSorted確認 |
| CHART-12 テンプレ未置換検出 | P0 | e2e_security (XSS検証) | 🟡 部分 | `{{VAR}}`正規表現検出 |

### Suite 3: TABLE (6) / Suite 4: MAP (10)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| TABLE-01 ランキング構造 | P1 | ❌ | ❌ 未実施 | ヘッダー3列/10行以上 |
| TABLE-02 数値妥当性 | P1 | ❌ | ❌ 未実施 | NaN/負数検出 |
| TABLE-03 検索結果テーブル | P1 | e2e_other_tabs (詳細分析) | 🟡 部分 | #comp-results確認 |
| TABLE-04 資格一覧日本語 | P2 | ❌ | ❌ 未実施 | 日本語正規表現 |
| TABLE-05 KPIカンマ区切り | P2 | e2e_other_tabs (KPI抽出) | 🟡 部分 | 1000以上のカンマ検証 |
| TABLE-06 条件検索レスポンス | P2 | ❌ | ❌ 未実施 | #rarity-results |
| MAP-01 Leaflet初期化 | P1 | e2e_other_tabs | ✅ 完全 | - |
| MAP-02 マーカー検索 | P1 | e2e_other_tabs | 🟡 部分 | L.CircleMarker数え |
| MAP-03 クリック詳細 | P2 | ❌ | ❌ 未実施 | #jm-details-panel |
| MAP-04 市区町村カスケード | P2 | e2e_other_tabs (部分) | 🟡 部分 | 北海道vs東京diff |
| MAP-05 ピン止め | P2 | ❌ | ❌ 未実施 | PINボタン→カード |
| MAP-06 全画面モード | P2 | ❌ | ❌ 未実施 | ESC復帰 |
| MAP-07 コロプレス切替 | P2 | ❌ | ❌ 未実施 | TypeError 0 |
| MAP-08 地域統計パネル | P2 | ❌ | ❌ 未実施 | #jm-region-stats |
| MAP-09 企業マーカートグル | P2 | ❌ | ❌ 未実施 | - |
| MAP-10 ビューポート再読み込み | P3 | ❌ | ❌ 未実施 | setZoom(5) |

### Suite 5: NAV (8) / Suite 6: FILTER (8)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| NAV-01 全8タブ存在 | P1 | e2e_other_tabs (5タブ) | 🟡 部分 | 8タブcount==8 |
| NAV-02 切替コンテンツ変化 | P0 | e2e_other_tabs | ✅ 完全 | - |
| NAV-03 activeクラス | P2 | ❌ | ❌ 未実施 | 単一アクティブ |
| NAV-04 キャッシュ動作 | P2 | ❌ | ❌ 未実施 | リクエスト監視 |
| NAV-05 キャッシュ無効化 | P2 | ❌ | ❌ 未実施 | - |
| NAV-06 高速切替安定性 | P2 | ❌ | ❌ 未実施 | 200ms間隔 |
| NAV-07 自動ロード | P1 | e2e_other_tabs (前提) | 🟡 部分 | プレースホルダ検出 |
| NAV-08 ジョブタイプ切替 | P2 | ❌ | ❌ 未実施 | job-type-select |
| FILTER-01 都道府県→市区町村 | P1 | e2e_other_tabs | 🟡 部分 | 名古屋確認 |
| FILTER-02 雇用形態フィルタ | P1 | ❌ | ❌ 未実施 | 全件vs正社員差 |
| FILTER-03 2階層ドロップダウン | P2 | ❌ | ❌ 未実施 | ftype-btn |
| FILTER-04 産業分類県連動 | P2 | ❌ | ❌ 未実施 | - |
| FILTER-05 近辺エリア | P2 | ❌ | ❌ 未実施 | comp-nearby |
| FILTER-06 グローバル県フィルタ | P1 | ❌ | ❌ 未実施 | pref-select変更 |
| FILTER-07 産業2階層ツリー | P2 | ❌ | ❌ 未実施 | ind-major-cb |
| FILTER-08 リセット表示 | P2 | ❌ | ❌ 未実施 | 空フィルタ |

### Suite 7: COMPANY (7) / Suite 8: RESPONSIVE (5)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| COMPANY-01 初期表示 | P2 | e2e_other_tabs | ✅ 完全 | - |
| COMPANY-02 API v1認証不要 | P2 | e2e_api_excel | ✅ 完全 | - |
| COMPANY-03 プロフィール | P2 | e2e_other_tabs | 🟡 部分 | - |
| COMPANY-04 レポートページ | P2 | e2e_print_verify, e2e_report_insight | ✅ 完全 | - |
| COMPANY-05 近隣API | P2 | e2e_api_excel | ✅ 完全 | - |
| COMPANY-06 求人一覧API | P2 | e2e_api_excel | ✅ 完全 | - |
| COMPANY-07 /health | P2 | ❌ | ❌ 未実施 | 認証なし200 |
| RESPONSIVE-01〜05 | P3 | ❌ | ❌ 未実施 | 375pxビューポート関連全般 |

### Suite 9: ERROR (6) / Suite 10: FLOW (5)

| ID | 優先度 | 実施スクリプト | カバレッジ | 未実施項目 |
|----|--------|---------------|-----------|-----------|
| ERROR-01 存在しない県 | P2 | e2e_security (境界) | 🟡 部分 | /api/competitive/filter |
| ERROR-02 HTML構造 | P2 | e2e_other_tabs (部分) | 🟡 部分 | fetch('/tab/overview') |
| ERROR-03 大量マーカー | P3 | ❌ | ❌ 未実施 | 30秒以内 |
| ERROR-04 コンソールエラー全タブ | P0 | ❌ | ❌ 未実施 | 全8タブTypeError 0 |
| ERROR-05 ローディング表示 | P3 | ❌ | ❌ 未実施 | 遅延ルート |
| ERROR-06 同時リクエスト競合 | P3 | ❌ | ❌ 未実施 | 100ms間隔 |
| FLOW-01 採用担当ワークフロー | P1 | e2e_other_tabs (部分) | 🟡 部分 | 5ステップ通し |
| FLOW-02 地図探索 | P1 | ❌ | ❌ 未実施 | 地図→ピン止め |
| FLOW-03 条件検索 | P2 | ❌ | ❌ 未実施 | rarity |
| FLOW-04 全8タブ基本表示 | P0 | e2e_other_tabs (5タブ) | 🟡 部分 | 残り3タブ |
| FLOW-05 レポート出力 | P3 | e2e_report_insight, e2e_print_verify | ✅ 完全 | - |

---

## 3. P0/P1 完全カバレッジ確認

### P0テスト（ブロッカー）カバレッジ不足

**E2E_TEST_PLAN.md P0テスト (14件)**:

| ID | 状態 | 対応必要度 |
|----|------|-----------|
| INFRA-001 /health db_connected | 🟡 部分 | 明示確認追加 |
| INFRA-002 静的ファイル | ❌ | 要追加 |
| AUTH-001〜003 | 🟡 部分 | 専用確認追加 |
| OV-001 | ✅ | - |
| OV-003 | ✅ | - |
| ANA-001 6サブタブ | 🟡 部分 | 要追加 |
| ANA-004 パーセンタイル昇順 | ❌ | **要追加（データ整合性）** |
| DIAG-002 | ✅ | - |
| CO-002 | ✅ | - |
| API-001/009/010 | ✅ | - |
| FILT-001 47都道府県 | ❌ | **要追加** |
| FILT-002 カスケード | 🟡 部分 | - |
| CROSS-001 4タブ総件数 | ❌ | **要追加（データ整合性）** |

**E2E_TEST_PLAN_V2.md P0テスト (11件)**:

| ID | 状態 | 対応必要度 |
|----|------|-----------|
| AUTH-01〜05 | 🟡 部分 | AUTH-03/05は要追加 |
| CHART-01 | ✅ | - |
| CHART-02 | 🟡 部分 | - |
| CHART-03 産業別グループ棒 | ❌ | **要追加** |
| CHART-05 canvas非空白 | ❌ | **要追加（視覚検証）** |
| CHART-06 JSONパース | ❌ | **要追加** |
| CHART-12 未置換検出 | 🟡 部分 | **要追加（正規表現）** |
| NAV-02 切替変化 | ✅ | - |
| ERROR-04 全タブconsoleエラー | ❌ | **要追加** |
| FLOW-04 8タブ表示 | 🟡 部分 | 3タブ追加必要 |

### P1テスト 未カバー一覧（主要項目のみ）

| 領域 | 未カバーID |
|------|-----------|
| 認証 | AUTH-004 Set-Cookie, AUTH-06 ログアウト後 |
| 基盤 | INFRA-003 GeoJSON 47県 |
| 概要 | OV-004 産業ツリー, OV-006 pie合計 |
| 求人条件 | WS-001〜002 |
| 採用動向 | DEMO-001 |
| 地図 | MAP-003座標範囲, MAP-006詳細 |
| 市場分析 | ANA-002/005/008/010/013/014 |
| 診断 | DIAG-004/005 高低条件 |
| 企業 | CO-003/004 |
| API | API-005 給与整合性 |
| フィルタ | FILT-003/004 |
| キャッシュ | CACHE-002 |
| 整合性 | CROSS-002/004 |
| Charts | CHART-001〜002, CHART-04/09 |
| パフォーマンス | PERF-001 |
| V2 FLOW | FLOW-01/02 |

---

## 4. 本セッション追加実装機能（計画書に無し）

以下はE2E_TEST_PLAN.md/V2.mdに記載がないが、2026-04-12実装のため追加テストが必要。

| 機能 | 実装日 | 実施スクリプト | 追加テスト項目 |
|------|--------|---------------|---------------|
| CSRF対策（Origin検証） | 2026-04-12 | e2e_security (CSRF) | ✅ 完全: POST時Origin不一致→403 |
| sanitize_tag_text() XSS強化 | 2026-04-12 | e2e_security (XSS 7種) | ✅ 完全: `<script>`/`<img onerror>`等 |
| 20MB bodyサイズ制限 | 2026-04-12 | e2e_security (大容量) | ✅ 完全: 21MB→413 Payload Too Large |
| /report/insight データ拡張 | 2026-04-12 | e2e_report_insight | 🟡 部分: 地域経済+労働力リスク項目の存在確認が必要 |
| 表紙ページ | 2026-04-12 | e2e_print_verify | 🟡 部分: PDFページ1が表紙かcover要素検証 |
| ダークモード | 2026-04-12 | ❌ | ❌ 未実施: prefers-color-scheme/trgl |
| ARIA属性 | 2026-04-12 | ❌ | ❌ 未実施: aria-label/role検証 |

**追加テスト提案**:

```python
# e2e_report_insight に追加
assert "地域経済" in html or "経済指標" in html
assert "労働力リスク" in html or "労働供給" in html

# e2e_print_verify に追加
# PDFページ1のテキスト抽出→「表紙」「レポート」等タイトル語を確認

# e2e_a11y_darkmode.py 新規作成
# - ARIA: [role=button], [aria-label] count >= N
# - ダークモード: emulateMedia(prefers-color-scheme='dark') → bg色検証
```

---

## 5. 最終確認テスト要件定義: `e2e_final_verification.py`

### 目的
デプロイ前に実施する「核心テスト」統合スクリプト。P0全件 + P1主要 + 本セッション追加機能の検証を10分以内で完了する。

### 実行環境要件

| 項目 | 値 |
|------|-----|
| ブラウザモード | headless=True |
| slow_mo | 100 ms |
| タイムアウト/テスト | 30秒 |
| 全体タイムアウト | 600秒 (10分) |
| 並列度 | Suite内シーケンシャル、Suite間は独立 |
| リトライ | P0は1回、P1以下はなし |

### 成功基準

| 優先度 | 目標合格率 | 失敗時の扱い |
|--------|-----------|-------------|
| P0 | 100% | デプロイブロック |
| P1 | 95%以上 | リリース判断要協議 |
| P2 | 80%以上 | 警告ログのみ |

### 含めるべき検証スイート

#### Suite A: インフラ基盤 (~30秒)

| 項目 | ソース | 優先度 |
|------|--------|--------|
| /health db_connected=true | INFRA-001 | P0 |
| /static/js/app.js 200 | INFRA-002 | P0 |
| /static/js/charts.js 200 | INFRA-002 | P0 |
| /api/prefectures 47件 | FILT-001 | P0 |
| 認証なし/tab/overview→302 | AUTH-05 (V2) | P0 |

#### Suite B: 認証フロー (~30秒)

| 項目 | ソース | 優先度 |
|------|--------|--------|
| 正常ログイン→ダッシュボード | AUTH-02 | P0 |
| 不正パスワード→エラー表示 | AUTH-03 (V2) | P0 |
| 許可外ドメイン→拒否 | AUTH-04 (V2) | P0 |
| ログアウト→/login | AUTH-06 (V2) | P1 |

#### Suite C: データ正確性 (~90秒)

| 項目 | ソース | 優先度 | 既存スクリプト参照 |
|------|--------|--------|-------------------|
| 総求人件数 460K-480K | OV-001 | P0 | e2e_other_tabs |
| 全国→東京都でKPI変化 | OV-003 | P0 | e2e_other_tabs |
| 4タブで総件数一致 | CROSS-001 | P0 | **新規** |
| pie合計=総件数 | OV-006 | P1 | **新規** |
| 給与P25<P50<P75<P90 | ANA-004 | P0 | **新規** |
| 最低賃金違反率 0-20% | ANA-010 | P1 | **新規** |

#### Suite D: Charts描画 (~60秒)

| 項目 | ソース | 優先度 |
|------|--------|--------|
| ドーナツ data存在 | CHART-01 | P0 |
| canvas非空白ピクセル | CHART-05 | P0 |
| JSONパースエラー0 | CHART-06 | P0 |
| `{{VAR}}`未置換0（6タブ） | CHART-12 | P0 |
| フィルタ→データ変化 | CHART-09 | P1 |

#### Suite E: タブ/ナビゲーション (~60秒)

| 項目 | ソース | 優先度 |
|------|--------|--------|
| 8タブ存在 | NAV-01 | P1 |
| 全8タブで50文字以上 | FLOW-04 | P0 |
| 全タブでTypeErrorなし | ERROR-04 | P0 |
| ANA 6サブタブ200 | ANA-001 | P0 |

#### Suite F: API v1 (~30秒)

| 項目 | ソース | 優先度 | 既存 |
|------|--------|--------|------|
| /api/v1/companies 正常 | API-001 | P0 | e2e_api_excel |
| SQLi耐性 | API-009 | P0 | e2e_security |
| XSS耐性 | API-010 | P0 | e2e_security |
| 2文字制限 | API-002 | P1 | e2e_api_excel |

#### Suite G: 診断/企業検索 (~45秒)

| 項目 | ソース | 優先度 | 既存 |
|------|--------|--------|------|
| 診断標準条件→グレード返る | DIAG-002 | P0 | e2e_other_tabs |
| 企業2文字検索→結果 | CO-002 | P0 | e2e_other_tabs |
| 診断高条件→S/A | DIAG-004 | P1 | **新規** |
| 診断低条件→C/D | DIAG-005 | P1 | **新規** |

#### Suite H: 地図 (~45秒)

| 項目 | ソース | 優先度 |
|------|--------|--------|
| Leaflet初期化 | MAP-01 | P1 |
| マーカー座標範囲 (lat 20-46, lon 122-154) | MAP-003 | P1 |
| 市区町村カスケード | MAP-04 | P2 |

#### Suite I: レポート/セキュリティ (~60秒)

| 項目 | ソース | 優先度 | 既存 |
|------|--------|--------|------|
| /report/insight 200 + ECharts | FLOW-05 | P1 | e2e_report_insight |
| /report/survey CSVアップロード | SV-001 | P1 | e2e_report_survey |
| CSRF Origin検証 | **本セッション追加** | P0 | e2e_security |
| XSS 7種サニタイズ | **本セッション追加** | P0 | e2e_security |
| 20MB body制限 | **本セッション追加** | P1 | e2e_security |
| PDF生成成功 | **本セッション追加** | P1 | e2e_print_verify |

### 実装フレーム

```python
# e2e_final_verification.py 構造
SUITES = [
    ("A_infra",     run_suite_a, 30,  "P0"),
    ("B_auth",      run_suite_b, 30,  "P0"),
    ("C_data",      run_suite_c, 90,  "P0"),
    ("D_charts",    run_suite_d, 60,  "P0"),
    ("E_navigation",run_suite_e, 60,  "P0"),
    ("F_api",       run_suite_f, 30,  "P0"),
    ("G_diagnostic",run_suite_g, 45,  "P1"),
    ("H_map",       run_suite_h, 45,  "P1"),
    ("I_reports",   run_suite_i, 60,  "P0"),
]
# 合計: 450秒 = 7分30秒 (10分以内)

# レポート出力形式
"""
=== E2E Final Verification Report ===
Date: YYYY-MM-DD HH:MM
Environment: https://hr-hw.onrender.com
Total: N tests | Duration: Ns

[P0] PASS: X/Y (100% required)
[P1] PASS: X/Y (95% required)
[P2] PASS: X/Y (80% required)

Verdict: ✅ DEPLOY OK / ❌ BLOCK
"""
```

### カバレッジ目標

| 指標 | 本セッション現状 | e2e_final_verification実施後 |
|------|-----------------|----------------------------|
| E2E_TEST_PLAN.md P0カバー率 | 10/14 (71%) | 14/14 (100%) |
| E2E_TEST_PLAN.md P1カバー率 | ~25/35 (71%) | 33/35 (94%) |
| E2E_TEST_PLAN_V2.md P0カバー率 | 6/11 (55%) | 11/11 (100%) |
| E2E_TEST_PLAN_V2.md P1カバー率 | ~12/20 (60%) | 19/20 (95%) |
| 本セッション追加機能 | 5/7 | 7/7 |

---

## 6. まとめ

### 現状のカバレッジ

- **完全カバー**: API v1 / セキュリティ / レポート生成系は既存7スクリプトでほぼ網羅
- **部分カバー**: 認証・基本UI・フィルタは他スクリプトの副作用でカバー
- **未カバー**: 市場分析ANA-002〜014（14件P1）/ 地図詳細MAP-003〜010 / クロス整合性CROSS-001〜004 / パフォーマンスPERF / レスポンシブRESPONSIVE が空白

### 優先対応項目

1. **P0未カバー (最優先)**: CROSS-001, ANA-004, CHART-05, CHART-06, CHART-12, ERROR-04, FILT-001
2. **P1主要**: ANA系データ妥当性、DIAG-004/005、FLOW-01、CHART-04/09
3. **追加機能検証**: ダークモード/ARIA（e2e_a11y_darkmode.py 新規作成推奨）

### 提案スクリプト構成（次ステップ）

| 新規スクリプト | 目的 | 想定実行時間 |
|---------------|------|-------------|
| `e2e_final_verification.py` | P0/P1核心統合 | 7-10分 |
| `e2e_ana_subtabs.py` | ANA-002〜014 専用 | 3-5分 |
| `e2e_cross_integrity.py` | CROSS-001〜004 | 2-3分 |
| `e2e_a11y_darkmode.py` | ARIA/ダークモード | 1-2分 |
