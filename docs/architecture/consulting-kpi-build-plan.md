# コンサルKPI（管理職の画面）作り直しの段取り

更新日: 2026-10-17
正本: 見本 `consulting-kpi-mock.html`、要件 `consulting-requirements.md`（§5・§7-0）

## 方針

- 新しい画面は React の `/app/consulting`。今の `/consulting`（HTMX・シート）は残す。ナビは変えない（URL で開く）。
- 主役は **接触・MTG の途絶**。分析レポート（2026 上期 改訂版）の指標（初回継続・初回の月応募 2 件以上・面接数・初回 NPS・継続意向）を足す。
- データ: HubSpot の取引の項目（案件管理の 5 分キャッシュを共用）、応募の記録（案件管理の 60 分キャッシュ）、シートの MTG・電話・継続回数（コンサルKPI の SheetStore を共用）。新しい DB は作らない。
- 判定・定義は 1 か所（`cs_dashboard` と `job_copy_live` の関数を共用）。

**今のコンサルKPI にある機能は消さない。** 新しい画面に移し終えるまで `/consulting` はそのまま動かす。移す先: 今日・案件一覧・案件の詳細 → 案件管理、チームと担当・接触の推移・担当の交代 → チーム／担当者、満了と継続 → 満了と継続、成果と継続 → 月の振り返り、いま見るべき顧客・本部アプローチ → 顧客・本部、電話 → 担当者、記録と数字の信頼度 → 月の振り返りの畳み。

## 段階

| 段 | 作業 | 並列 | 触るファイル（重ならないように分ける） |
|---|---|---|---|
| 0 | 土台: Rust のモジュール・API の型と空の応答・React の画面とタブの枠・共通の印の判定・架空データの表示モード | 1 本 | `src/handlers/consulting_v2/{mod,routes,types,signals,data}.rs`、`frontend/src/screens/consulting/{ConsultingScreen,api,model}.tsx/ts` |
| 1-A | チームのタブ（上の数字・担当者の表） | 並列 | `consulting_v2/team.rs`、`screens/consulting/TeamTab.tsx` |
| 1-B | 担当者のタブ（今の「接触の推移」「担当の交代」も移す） | 並列 | `consulting_v2/person.rs`、`screens/consulting/PersonTab.tsx` |
| 1-C | 満了と継続のタブ | 並列 | `consulting_v2/renewal.rs`、`screens/consulting/RenewalTab.tsx` |
| 1-D | 月の振り返りのタブ | 並列 | `consulting_v2/monthly.rs`、`screens/consulting/MonthlyTab.tsx` |
| 1-E | 顧客・本部のタブ（今の「いま見るべき顧客」の注力顧客と「本部アプローチ」の事業所どうしの差を移す） | 並列 | `consulting_v2/accounts.rs`、`screens/consulting/AccountsTab.tsx` |
| 2 | つなぎ・確認: 本番相当のデータでの見た目、E2E、マージ（CRM 側と 1 本ずつ） | 1 本 | — |

各段は Codex が実装し、Claude が依頼・画面の確認・マージをする。
