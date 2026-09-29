import { describe, expect, it } from 'vitest';
import { hasQueryFilters, readQueryFilters } from './filters';

describe('readQueryFilters', () => {
  it('distinguishes absent keys (null) from empty values', () => {
    expect(readQueryFilters('')).toEqual({ pref: null, muni: null, ind: null, jt: null });
    expect(hasQueryFilters(readQueryFilters(''))).toBe(false);
    const q = readQueryFilters('?pref=&ind=a,%20b,,c');
    expect(q).toEqual({ pref: '', muni: null, ind: ['a', 'b', 'c'], jt: null });
    expect(hasQueryFilters(q)).toBe(true);
  });
});
