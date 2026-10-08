import { describe, expect, it } from 'vitest';
import { crmView } from './crmView';

describe('crmView', () => {
  it.each([
    ['', 'queue'],
    ['?mode=fixture', 'queue'],
    ['?view=queue&mode=fixture', 'queue'],
    ['?view=bogus', 'queue'],
    ['?view=moc', 'moc'],
    ['?view=calling', 'moc'],
    ['?view=reference', 'reference'],
    ['?view=single', 'single'],
  ])('%s -> %s', (search, want) => {
    expect(crmView(search)).toBe(want);
  });
});
