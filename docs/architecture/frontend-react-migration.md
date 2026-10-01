# HR_HR フロントエンド React 移行方針

更新日: 2026-09-29

## 目的

HR_HR のフロントエンドを、現在の HTMX / Vanilla JavaScript 中心構成から
React + TypeScript + Vite へ段階的に移行するための正本。

バックエンドの Rust / Axum は維持する。

## 結論

```text
Frontend
React
TypeScript
Vite
ECharts
Leaflet
必要に応じて TanStack Query

        │ JSON API
        ▼

Backend
Rust / Axum
Authentication / Session
Authorization
Business Logic
Aggregation
External API
Data Access
```

責務は以下に固定する。

- React = UI / Client State / Interaction
- Rust = Server / Business Logic / Security / Data Access
- 認証ロジックは Rust 側を正とし、React 側へ移さない
- Next.js 等のフルスタック React Framework は現時点では導入しない
- 全面一括リプレイスは行わない

## なぜ今見直すのか

HTMX 採用当初の判断は合理的だった。
当時の主目的は、大量データを扱う分析ダッシュボードを NiceGUI より高速かつ単純な構成で提供することだった。

現在の HR_HR は以下まで拡張している。

- 求人市場分析
- Indeed 分析
- SalesNow 企業分析
- 地図 / Leaflet
- Agoop 人流
- ECharts
- 採用診断
- 媒体分析
- CSV upload
- 求人票生成
- AI 分析
- コンサル KPI
- 営業 KPI
- 架電クオリティ
- レポート
- スカウト関連
- 今後追加する Headless CRM / Zoom Phone

現在は「サーバが HTML を生成して見せる Dashboard」より
「ブラウザが状態を持ちながら動く Data / Operations Application」に近い。

## 現状の構造的課題

Client State が以下へ分散している。

- DOM value / class
- HTML
- Rust Session
- window 変数
- Leaflet instance
- ECharts instance
- JavaScript Object
- 独自 Cache

また、HTMX の DOM swap 後に以下を再構築する必要がある。

- Event Listener
- HTMX binding
- ECharts
- Leaflet
- JavaScript State

現在すでに、HTMX Partial / Vanilla JS / fetch / JSON API / Full HTML / dynamic import / iframe が共存している。

React 導入の目的は HTMX を否定することではなく、
Client State と UI Lifecycle を構造化することである。

## Rust の安全性思想を Frontend へ拡張する

Rust の compile-time safety に加え、Frontend でも TypeScript により以下を build 前に検出する。

- 型不一致
- 存在しない property
- Props 不足
- Import error
- API response shape の不一致

将来的には Rust API schema から TypeScript 型を自動生成する。

推奨:

```text
Rust structs / OpenAPI
        ↓
TypeScript types
        ↓
React
```

Backend 変更で Frontend contract が壊れた場合は CI で検出し Deploy を止める。

## CI / Deploy Gate

最低限:

```text
Rust
- cargo fmt
- cargo clippy
- cargo test
- cargo build

Frontend
- TypeScript check
- ESLint
- unit test
- Vite build

E2E
- Playwright
```

## Performance 原則

React 化は大量データを Browser に移すことを意味しない。

禁止 / 原則回避:

- 全画面を initial bundle に含める
- Leaflet を initial load する
- 全 ECharts を初期化する
- 大量データを Browser へ一括送信する
- 巨大 table の全行 DOM 化
- 不要な global state

画面単位 lazy loading / code splitting を前提とする。

## 移行方法

Strangler Pattern。

```text
Phase 0
React / TypeScript / Vite 基盤
API client / API contract
共通 Auth 前提
CI

Phase 1A
小さい既存画面で移行方法を検証
例: recruitment_diag

Phase 1B
Headless CRM を React で新規実装

Phase 2+
既存画面を順次 React 化

最終
共通 App Shell / Navigation
HTMX 依存を段階縮小
```

Headless CRM 自体を React 導入可否の実験台にはしない。
先に小さな既存画面で React + Rust API + CI + E2E の経路を成立させる。
(2026-09-29 補足: Phase 0 がマージされ、ビルド・配信・型生成・CI の経路が成立したので、Headless CRM の React 画面は Phase 1A と並行してよい。ADR-014 の補足を参照。)
(2026-09-29 補足: 上の図の「最終: 共通 App Shell / HTMX 依存を段階縮小」は、全画面移行の計画で次のように具体化した。App Shell は W2 で先に作り、終点(W10)で HTMX 依存を撤去する。)

### 全画面移行の計画(2026-09-29)

全画面を移す波・完了条件・撤去手順の正本は `docs/architecture/react-full-migration-plan.md`。要約:

| 波 | 対象 |
|---|---|
| W1 | Phase 1A 採用診断 + Headless CRM の React 画面(並行) |
| W2 | 営業KPI + App Shell v1 |
| W3 | 職種辞典・資格辞書・キーワード需要・ガイド(セッションのフィルタを読まない画面) |
| W4 | 地域分析・企業検索・採用市場(HTML partial の JSON 化が中心) |
| W5 | 地図(Leaflet) |
| W6 | 媒体分析の画面部分 |
| W7 | コンサルKPI(UI/UX 改修が落ち着いてから) |
| W8 | 架電クオリティ・consult・admin / my・求人票作成 |
| Wh | 非表示の画面(React に移し、`hidden` で隠す) |
| W9 | レポート(React の印刷用画面 `/app/print/*` へ) |
| W10 | HTMX・tabcache・旧シェル・旧テンプレート・`/api/set_*`・旧 `/tab/*`・CDN・precompiled CSS を撤去し、CSP から `'unsafe-inline'` を外す |

W2 以降は担当チームを分けて並行で進める。

先に作る共通基盤(platform-team が作り、Headless CRM と既存画面の移行で共用する):

- App Shell: ナビは Rust の `/api/nav` が返し、旧シェルと共用する。非表示画面の隠し方は `hidden` フラグに統一する。
- ヘッダーフィルタ: 移行中はセッションを正とし、URL クエリにも書く。最後は URL を正にする。
- API client: POST / upload / ジョブのポーリングに対応する。
- 認証: `/api/*` は 401 を JSON で返す(`HX-Request` の無い JSON 要求だけ)。React からの POST には `X-Requested-With` を必須にする。
- 共通部品: DataTable / KpiCard / Note / EChart / LeafletMap。
- スタイル: ビルド型 Tailwind。旧画面の CSS と二重に読み込まない。
- 検証: PR ごとの E2E で、旧画面と新画面の表示値が一致することを確かめる。
- HTML partial を JSON にする標準手順。

範囲: レポートも非表示の画面も含めて全画面を移す。非表示の画面は削除せず、`/api/nav` の `hidden` フラグで隠す。login だけは Rust に残す(ADR-002 追記)。

## SPA 化判断の基準

Feature 数ではなく Client State の複雑さで判断する。

HTMX が得意:
- Server state が正
- 単純 CRUD
- 表示更新中心
- Client State が小さい

React が有利:
- 複数 Record の同時状態
- Real-time
- Zoom Phone の常駐状態
- optimistic UI
- Undo / Redo
- drag & drop
- 複数画面で共有する Client State

## 非採用

### Next.js
現時点では不要。Rust が Backend / Server の責務を持つため。

### Leptos
Frontend まで Rust 統一は魅力だが、ECharts / Leaflet / DataGrid 等の JS ecosystem との interop が追加複雑性になる。

### 全面一括 React 化
回帰リスクが高いため行わない。
