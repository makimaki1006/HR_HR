# Recorded applicant reasons: incremental read plan

2026-10-05. Keep Rust/Axum, HubSpot System of Record, existing Google OIDC and explicit job-copy allowlist. No write or new store.

1. Read confirmed Applicant 0-421 properties: oubodouki, ouboriyuu_baitaikisai, ouboriyuu_hiaringu after existing customer/listing authorization.
2. Deduplicate applicant IDs. Preserve source-property missing/null versus blank. Property values are not applicant counts. Opaque scoped IDs; cap 100 descriptions and 2000 Unicode characters with explicit truncation.
3. Valid yingmuri remains optional application date. No invented collection time, copy-version association or causal claim; basis recorded_applicant_reason and version null.
4. Optional strict snapshot fields preserve old compatibility. Joint demographic cells originate from individuals, never marginal cross-products. HRH performance must match job and have nonoverlapping valid periods.
5. Synthetic and HTTP mock checks cover counts, bounds and authorization. Offline refresh needs requested demographic keys and explicit verified reverse associations for attribution, preserving media observations.

Original internal text may contain personal information; existing authorized internal access permits review. No anonymization claim. Collapse text and exclude default customer report/printing. No separate identity fields. Only aggregate counts/static errors in operator stdout.
