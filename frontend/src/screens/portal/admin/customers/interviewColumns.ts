/** 見出しから面接の日と時間の列を当てる。見出しの空白（全角含む）は無視して比べる。 */

const squash = (s: string): string => s.replace(/\s/g, '');

const DATE_NAMES = ['面接予定日', '面接日時', '面接日', '面接予定', '面接'];
const TIME_NAMES = ['時間', '面接時間', '面接時刻', '時刻'];
const NOT_DATE = ['結果', '辞退', '済', '時間', '時刻', '担当', '場所'];

export interface InterviewGuess {
  /** 面接の日の列の見出し（当たらなければ空）。 */
  date: string;
  /** 面接の時間の列の見出し（当たらなければ空）。 */
  time: string;
}

export function guessInterviewColumns(headers: string[]): InterviewGuess {
  const keyed = headers.map((h, i) => ({ h, k: squash(h), i }));
  let date: { h: string; i: number } | undefined;
  for (const name of DATE_NAMES) {
    const hit = keyed.find((x) => x.k === name);
    if (hit) { date = hit; break; }
  }
  date ??= keyed.find((x) => x.k.includes('面接') && !NOT_DATE.some((w) => x.k.includes(w)));

  const times = keyed.filter((x) => TIME_NAMES.includes(x.k) && x.h !== date?.h);
  let time: { h: string } | undefined;
  if (date) {
    const right = times.find((x) => x.i === date.i + 1);
    if (right) time = right;
  }
  if (!time) {
    for (const name of TIME_NAMES) {
      const hit = times.find((x) => x.k === name);
      if (hit) { time = hit; break; }
    }
  }
  return { date: date?.h ?? '', time: time?.h ?? '' };
}
