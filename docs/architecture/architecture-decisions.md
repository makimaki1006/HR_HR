# Architecture Decision Records — Frontend / Headless CRM

更新日: 2026-09-28

この文書は既決事項の短い索引。
詳細は以下を参照。

- `docs/architecture/frontend-react-migration.md`
- `docs/architecture/headless-crm-design.md`

---

## ADR-001 — Rust / Axum を維持する

Status: Accepted

Backend の Rust / Axum は置き換えない。

理由:
- Business Logic
- Data processing
- Security
- API integration
- compile-time safety

を既に担っており、現在の問題の中心は Backend ではなく Frontend の state / lifecycle 複雑化である。

---

## ADR-002 — Frontend 標準を React + TypeScript + Vite へ段階移行する

Status: Accepted

HTMX 採用当初は合理的だったが、HR_HR が Data / Operations Application 化し Client State が増えているため。

全面一括 replacement はしない。

---

## ADR-003 — Headless CRM は React で新規実装する

Status: Accepted

理由:
- Zoom Phone 常駐 state
- selected Contact / Company / Deal
- input draft
- sync state
- retry state

など Client State が明確に存在する。

---

## ADR-004 — HR_HR と Headless CRM は同じ Rust / Axum / Render Application に載せる

Status: Accepted

別 Render Service を前提としない。

Frontend technology は既存画面と CRM で段階的に異なってよい。

---

## ADR-005 — HubSpot を System of Record とする

Status: Accepted

CRM master を独自 DB に複製しない。

Contact / Company / Deal / Property / Activity / Workflow は HubSpot を正とする。

---

## ADR-006 — 独自 CRM DB を作らない

Status: Accepted

DB 追加条件は「HubSpot / Zoom に置くべきではない HR_HR 固有の永続 data が発生した場合」。

ユーザー数増加だけでは追加しない。

---

## ADR-007 — Zoom Phone 通話一次情報を HubSpot Call Activity へ反映する

Status: Accepted

現行 HubSpot × Zoom Phone の実利用項目を棚卸しし、必要な項目だけ再現する。

---

## ADR-008 — HubSpot Workflow を残す

Status: Accepted

Rust に Workflow business logic を全面複製しない。

Property update → existing Workflow を基本とする。

---

## ADR-009 — Durable Retry / Pending Sync を持つ

Status: Accepted

HubSpot の一時障害時に、人が入力したデータを失わない。

ただし Queue は CRM copy ではない。
未反映 operation のみ保持する。

具体 storage は未決定。

---

## ADR-010 — CREATE 系は Idempotency を必須とする

Status: Accepted

Call Activity / Task / Note 等の二重作成を防ぐ。

operation_id / zoom_call_log_id 等を利用する。

---

## ADR-011 — Rust Backend を RBAC の境界とする

Status: Accepted

HubSpot Service credential は Browser に渡さない。

BPO が書ける object / property / operation を Rust 側で制限する。

---

## ADR-012 — HubSpot Deep Link を UI に提供する

Status: Accepted

Contact / Company / Deal の object ID から「HubSpotで開く」を提供する。

---

## ADR-013 — System Log と CRM Activity を分離する

Status: Accepted

- CRM業務履歴 → HubSpot
- Zoom通話一次情報 → Zoom / HubSpotへ反映
- Rust system error / retry → Render Logs

---

## ADR-014 — React 移行は小さい既存画面で先に検証する

Status: Accepted

Headless CRMを React 導入可否の実験台にしない。

React + Rust API + TypeScript contract + CI + Playwright の経路を小さな既存画面で先に成立させる。

---

## ADR-015 — Next.js を現時点では導入しない

Status: Accepted

Rust / Axum が Backend / Server responsibility を持つため。
React + TypeScript + Vite を採用する。

---

## ADR-016 — HubSpot pricing 永続性を設計前提にしない

Status: Accepted

API / Seat 条件が将来変わって採算が悪化した場合、Core Seat 運用へ戻せる。

Vendor pricing を予測し切ることより、設計の可逆性を維持する。
