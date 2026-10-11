import { expect, it } from 'vitest';
import { displayCode, holidayValue } from './copyDisplay';
it('hides unverified trial, pay and employment codes while retaining readable original labels', () => {
  for (const [key, value] of [['試用・研修の有無', '2'], ['試用・研修時の雇用条件', '2'], ['試用・研修期の給与のタイプ', '3'], ['試用・研修期の雇用形態', '6'], ['給与形態', '３'], ['雇用形態', '-1']]) expect(displayCode(key ?? '', value ?? '')).toBe('確認が必要');
  expect(displayCode('給与形態', '月給')).toBe('月給');
  expect(displayCode('基本給与 最小', '213000')).toBe('213000');
});
it('matches holidays in either supplement group and never assumes the first free field is a holiday', () => {
  const fields = new Map([['自由項目1のタイトル', '応募案内'], ['自由項目1の内容', '電話で応募'], ['仕事情報補足2のタイトル', '休日休暇'], ['仕事情報補足2の内容', '土日・夏季休暇']]);
  expect(holidayValue(fields)).toBe('土日・夏季休暇');
  fields.set('仕事情報補足2のタイトル', '待遇');
  expect(holidayValue(fields)).toBeUndefined();
  fields.set('自由項目3のタイトル', '休日・休暇');
  fields.set('自由項目3の内容', '週休2日');
  expect(holidayValue(fields)).toBe('週休2日');
});
