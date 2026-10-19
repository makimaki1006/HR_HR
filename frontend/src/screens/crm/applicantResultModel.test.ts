// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { applicantDetailFixture } from './applicantFixture';
import { applicantChanges, datetimeInput } from './applicantResultModel';
describe('応募の保存本文', () => {
  it('JST の MM/DD を付けて改行で追記し、変えた項目とステージだけ送る', () => {
    const detail = applicantDetailFixture('90001');
    const changes = applicantChanges(detail, { renrakukakuninmokuteki: ' 条件を確認\n午後を希望 ', denwabangou: '000-0000-0000', taioushubetsu: 'done' }, 'fixture-stage-5', '2026-10-19');
    expect(changes).toEqual([
      { object: 'deal', name: 'taioushubetsu', base: null, value: 'done' },
      { object: 'deal', name: 'renrakukakuninmokuteki', base: '10/18 午後の連絡を希望', value: '10/18 午後の連絡を希望\n10/19 条件を確認\n午後を希望' },
      { object: 'deal', name: 'hs_pipeline_stage', base: '1431574804', value: 'fixture-stage-5' },
    ]);
    expect(applicantChanges(detail, { renrakukakuninmokuteki: '  ' }, detail.row.stage_id)).toEqual([]);
  });
  it('日時の時刻を JST のまま往復し、未編集の時刻は変更しない', () => {
    const detail = applicantDetailFixture('90001');
    detail.values.yakusokukadennichiji = '2026-10-19T05:45:00Z';
    expect(datetimeInput(detail.values.yakusokukadennichiji)).toBe('2026-10-19T14:45');
    expect(applicantChanges(detail, {yakusokukadennichiji: '2026-10-19T14:45'}, '')).toEqual([]);
    expect(applicantChanges(detail, {yakusokukadennichiji: '2026-10-20T09:30'}, '')).toEqual([{object:'deal',name:'yakusokukadennichiji',base:'2026-10-19T05:45:00Z',value:'2026-10-20T00:30:00.000Z'}]);
  });
});
