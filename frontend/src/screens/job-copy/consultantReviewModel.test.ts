import { describe, expect, it } from 'vitest';
import { observationWindowError } from './consultantReviewModel';
describe('consultant observation period', () => {
  it('does not invent a period when omitted', () => { expect(observationWindowError('', '')).toBeNull(); });
  it('requires both bounds', () => { expect(observationWindowError('2026-10-04', '')).not.toBeNull(); expect(observationWindowError('', '2026-10-04')).not.toBeNull(); });
  it('rejects reversed bounds', () => { expect(observationWindowError('2026-10-05', '2026-10-04')).not.toBeNull(); });
  it('allows same-day observation', () => { expect(observationWindowError('2026-10-04', '2026-10-04')).toBeNull(); });
  it('rejects calendar overflow rather than silently correcting it', () => { expect(observationWindowError('2026-02-30', '2026-03-04')).not.toBeNull(); });
  it('allows leap day only in a leap year', () => { expect(observationWindowError('2024-02-29', '2024-03-01')).toBeNull(); expect(observationWindowError('2026-02-29', '2026-03-01')).not.toBeNull(); });
});
