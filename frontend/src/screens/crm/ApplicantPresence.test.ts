import { describe, expect, it } from 'vitest';
import { phoneKey, presenceText, viewersOf } from './ApplicantPresence';

describe('開いている人の目印', () => {
  const viewers = [
    { applicant: '1', phone_key: phoneKey('090-1234-5678'), name: 'sato' },
    { applicant: '2', phone_key: phoneKey('+81 90 1234 5678'), name: 'suzuki' },
    { applicant: '3', phone_key: '', name: 'tanaka' },
  ];
  it('同じ応募と、同じ電話番号の別の応募を分けて出す', () => {
    expect(presenceText(viewersOf(viewers, '1', '09012345678'))).toBe('sato さんが開いています。suzuki さんが同じ電話番号の応募を開いています');
    expect(presenceText(viewersOf(viewers, '3', ''))).toBe('tanaka さんが開いています');
  });
  it('だれも開いていなければ何も出さない', () => {
    expect(presenceText(viewersOf(viewers, '9', '0311112222'))).toBe('');
  });
});
