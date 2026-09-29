# AGENTS.md

This repository is HR_HR.

Before changing Frontend architecture or implementing the Headless CRM, read:

- `CLAUDE.md`
- `docs/architecture/frontend-react-migration.md`
- `docs/architecture/headless-crm-design.md`
- `docs/architecture/architecture-decisions.md`

## Mandatory architecture rules

1. Keep Rust / Axum as the backend.
2. Frontend is moving gradually toward React + TypeScript + Vite.
3. Do not perform a full frontend rewrite in one change.
4. Headless CRM is implemented in React and lives inside the existing HR_HR application.
5. HubSpot is the CRM System of Record.
6. Do not introduce a separate CRM master database.
7. Do not add a new kind of persistent store (PostgreSQL / Redis / a new Turso database, etc.) merely because it is a common web-app pattern.
   Adding a table to an existing store already in operation (e.g. the audit Turso DB) is a valid candidate,
   but record the reason as an ADR when adopted.
8. Durable Retry / Pending Sync is required for HubSpot write failures, but its storage technology is not yet decided.
9. Keep existing HubSpot Workflows where possible; update HubSpot properties rather than duplicating all workflow logic in Rust.
10. All CREATE-style retryable operations must be idempotent.
11. HubSpot service credentials must never be exposed to the browser.
12. Rust backend is the authorization boundary for CRM writes.
13. Internal user login moves to Google Workspace OIDC (ADR-017). Do not add other identity providers
    or remove the existing external time-limited password access without a documented decision.
14. Add HubSpot deep links for relevant Contact / Company / Deal records.
15. System logs belong to application logging; business history belongs to HubSpot.
16. If proposing a change to an accepted architecture decision, do not silently replace it. Explain:
    - current decision
    - concrete problem
    - proposed change
    - benefit of changing
    - benefit of keeping current decision
    - migration/reversal cost

## Workflow before implementation

For a substantial CRM or frontend architecture task:

1. Inspect current code first.
2. Compare it with the architecture docs.
3. Identify contradictions or missing requirements.
4. Produce an implementation plan before editing.
5. Keep changes incremental.
6. Run Rust, frontend, and E2E checks appropriate to the changed area.

Do not infer approval for unrelated infrastructure changes.
