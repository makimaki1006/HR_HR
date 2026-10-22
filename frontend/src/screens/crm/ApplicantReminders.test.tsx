import { describe, expect, it } from 'vitest';
import { dueReminders } from './ApplicantReminders';
import { applicantRemindersFixture } from './applicantFixture';
import type { ApplicantRow } from '../../generated/ApplicantRow';

describe('dueReminders', () => {
  const now = Date.parse('2026-10-22T10:00:00+09:00');
  const row = (appointment: string): ApplicantRow => ({ id: '1', name: '架空', phone: '', deal: '', job: '', date: '', stage: '', stage_id: '', appointment, media: '' });
  it('約束架電日時の5分前から出し、過ぎても出し続ける', () => {
    expect(dueReminders([row('2026-10-22T10:05:00+09:00')], now)).toHaveLength(1);
    expect(dueReminders([row('2026-10-22T10:05:01+09:00')], now)).toHaveLength(0);
    expect(dueReminders([row('2026-10-22T09:00:00+09:00')], now)).toHaveLength(1);
  });
  it('ミリ秒の値も読み、日時の無いものは出さない', () => {
    expect(dueReminders([row(String(now + 4 * 60_000))], now)).toHaveLength(1);
    expect(dueReminders([row('')], now)).toHaveLength(0);
  });
  it('架空サンプルは1件だけ5分前を過ぎている', () => {
    expect(dueReminders(applicantRemindersFixture(now).items, now).map(r => r.name)).toEqual(['架空 みさき']);
  });
});
