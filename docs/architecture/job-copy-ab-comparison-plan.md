# Cross-record job A/B comparison

Different media/HubSpot job IDs may represent variants of the same recruitment. A comparison pairs two existing records without merging their IDs, histories, applications or performance. The consultant explicitly confirms the recruitment context; title/company text is not a canonical identity key.

## Incremental implementation

1. Add an A/B comparison tab to the existing React job-copy detail. A is the open record; B is selected from all loaded records independently of list filters. Select a published observation for each, review full text and images, and enter a comparison name/hypothesis.
2. Keep an explicit result scope: whole-record applicant aggregates or the selected observed version. Never present all-time demographics as selected-version results. Unknown attribution remains visible. Unpublished/received text is excluded.
3. Select an existing HR Hacker metric period independently for each job. Show actual impressions/clicks/media applications/cost and CTR/CPC/CPA, date overlap and duration. Do not prorate aggregate rows, mix HubSpot applicants into media conversion denominators, or attribute a period to a selected text version without evidence.
4. Show demographic counts and percentage-point differences. Reasons remain internal, escaped and collapsed, with unknown version attribution retained. No automatic winner, randomized allocation or causal inference is claimed.
5. Add calculation/interaction tests, typecheck/lint/build, and browser desktop/mobile/print checks. No backend/API/auth changes are required: comparison uses already-authorized data.

## Scope and persistence

This MOC comparison configuration lives only in React state; switching records or reloading clears it. No localStorage, new CRM master database or HubSpot writes. A future shared experiment record should live in HubSpot and associate both job records, with immutable observed-version references, dates, hypothesis and decisions. Defining the HubSpot schema and durable retry/idempotent writes is a separate integration step. Existing job-sync ownership and the single integration PR/window are preserved.

## Counterexamples

- Same title but unrelated openings: manual recruitment-context confirmation, no auto-merge.
- B hidden by list filter: selector uses all authorized loaded records.
- Missing applicants/metrics: unavailable, not zero.
- All-time counts versus selected copy: separate scopes, no cross-source denominator substitution.
- Different or disjoint metric dates: explicit periods, inclusive duration and overlap; no interpolated daily data.
- Historical images absent: display existing archive availability, never relabel current images as past originals.
- Multiple changed variables/unequal exposure: observed A/B comparison, no automatic treatment-effect or winner claim.
