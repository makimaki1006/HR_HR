import { describe, expect, it } from 'vitest';
import { inviteTokenFromHash } from './api';

describe('inviteTokenFromHash', () => {
  it('# の後ろの t= だけを読む', () => {
    expect(inviteTokenFromHash('#t=abc_DEF-123')).toBe('abc_DEF-123');
    expect(inviteTokenFromHash('#x=1&t=tok')).toBe('tok');
    expect(inviteTokenFromHash('')).toBe('');
    expect(inviteTokenFromHash('#t=<script>')).toBe('');
  });
});
