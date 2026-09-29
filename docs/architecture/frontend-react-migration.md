# HR_HR フロントエンド React 移行方針

更新日: 2026-09-28

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
