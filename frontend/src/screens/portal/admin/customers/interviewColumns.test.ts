import { describe, expect, it } from 'vitest';
import { guessInterviewColumns } from './interviewColumns';

describe('guessInterviewColumns', () => {
  it('面接予定日 と 時間', () => {
    expect(guessInterviewColumns(['応募者名', '状況', '面接予定日', '時間', '備考'])).toEqual({ date: '面接予定日', time: '時間' });
  });
  it('面接日時 のみ', () => {
    expect(guessInterviewColumns(['応募者名', '面接日時', '備考'])).toEqual({ date: '面接日時', time: '' });
  });
  it('面接日 と 面接時間', () => {
    expect(guessInterviewColumns(['応募者名', '面接日', '面接時間', '面接結果'])).toEqual({ date: '面接日', time: '面接時間' });
  });
  it('空白入りの見出しと、面接の結果の列は日にしない', () => {
    expect(guessInterviewColumns(['面接 結果', '面接　予定日', '時刻'])).toEqual({ date: '面接　予定日', time: '時刻' });
  });
  it('当たらなければ空', () => {
    expect(guessInterviewColumns(['応募者名', '面接結果', '時間'])).toEqual({ date: '', time: '時間' });
  });
});
