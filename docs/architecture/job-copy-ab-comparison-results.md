# Cross-record job A/B comparison results

The existing `/app/job-copy` React screen now has a **2求人のA/B比較** tab. A is the currently opened job; B is another authorized loaded job, including records hidden by the list's current filters. Different media IDs, HubSpot IDs and text bodies remain separate records and may be explicitly paired as the same recruitment context.

The consultant can name the comparison, enter a hypothesis, confirm the pairing and select a published observation independently for each job. Full text and available archived images can be reviewed alongside results. A/B settings survive tab navigation. Record changes/reload clear them: shared experiment persistence is not connected, and no HubSpot writes or new persistent store were introduced.

## Result boundaries

- Whole-record applicant totals/distributions and selected-observation attributed results are separate scopes. Unknown versions remain visible and are excluded from selected-version counts. Counts and percentages use their actual scope, including unknown attributes in distribution denominators.
- Independent HR Hacker metric periods show impressions, clicks, media applications, costs, CTR/CPC/CPA and click-to-media-application conversion. Inclusive duration, overlapping dates and CTR percentage-point difference are shown. No aggregate period is interpolated, and HubSpot application counts do not replace media denominators.
- A selected text observation is not automatically equated to the selected metric period. Exposures are not randomized; unequal conditions and different company/location/media are explicitly identified. There is no automatic causal effect or winning-variant declaration.
- Recorded reasons remain internal and collapsed. Version-unknown reasons are displayed in whole-record scope and are not allocated to a selected observation. Default printing excludes raw reasons.
- Real HR Hacker financial/click source ingestion remains unconnected. Metric demonstrations use clearly synthetic fixtures; the existing 36-job/320-application private snapshot and runtime settings were not modified.

## Validation

- Job-copy unit/interaction suite: 13 files, 149 tests passed, including three new A/B tests for separate scope, independent media denominators, inclusive/disjoint periods, different IDs/bodies and pairing confirmation reset.
- TypeScript, full frontend ESLint and Vite production build passed. Statistical-claims lint and CSS matcher self-tests passed. Rust/backend/auth routes were unchanged; previous-head Rust/CI results are not new-head validation.
- Complete candidate browser suite: 17 passed in 19.4 seconds, including two new A/B cases. Checks include text content, image decode, exact numeric results, list-filter-independent B selection, tab state retention and print exclusion. Five desktop and 375px mobile captures were visually inspected and remain private local audit artifacts. New A/B images are synthetic raster fixtures; existing image-regression cases retain captured-source checks.

New-head CI status belongs to PR #58. Merge/deployment continues through the existing single integration contact.

## Future shared experiment contract

Persist experiment metadata in HubSpot after defining its schema and write/retry contract: comparison identity/name, company/recruitment context, A/B job associations, frozen observation references, measurement scope/periods, hypothesis and consultant decision. Keep original job records and their eight-digit media IDs for source ingestion. A comparison identity groups variants; it does not replace source record IDs or silently deduplicate applicants across jobs.
