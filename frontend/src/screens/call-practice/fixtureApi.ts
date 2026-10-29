// ?mode=fixture: サーバもお客さん役も呼ばず、決まった返事で最後まで動かす (画面の確認と E2E 用)。
// 返事は熱い型の架空の人物。結果の数字は、送った発言の数など画面の操作から決まる。

import type { PracticeResult } from '../../generated/PracticeResult';
import type { PracticeTurnResponse } from '../../generated/PracticeTurnResponse';
import type { TalkTurn } from '../../generated/TalkTurn';
import type { PracticeApi } from './api';

export const FIXTURE_OPENING = 'はい、ではその日でお願いします';
export const FIXTURE_REPLIES: readonly string[] = [
  'ドライバーを2名、3か月以内に採りたいですね',
  '前任が辞めてしまって、早く埋めたいんです',
  '月額定額なら、前向きに検討できます',
  '最終的には社長が決めます。当日も同席できます',
];
const FIXTURE_FALLBACK = 'はい、そうですね';
const FIXTURE_CLOSING = '失礼します';

const BAND_ROWS = [
  { band: '1〜4 問', rate_text: '8.1% (31 / 382)' },
  { band: '5〜7 問', rate_text: '6.0% (47 / 783)' },
  { band: '8〜10 問', rate_text: '3.9% (38 / 987)' },
  { band: '11 問以上', rate_text: '4.0% (21 / 528)' },
];

export function fixtureBand(n: number): string {
  if (n <= 4) return '1-4';
  if (n <= 7) return '5-7';
  if (n <= 10) return '8-10';
  return '11+';
}

export function createFixtureApi(delayMs = 15): PracticeApi {
  const transcript: TalkTurn[] = [];
  let scriptSeen = false;
  let result: PracticeResult | null = null;
  const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

  return {
    config: () => Promise.resolve({ enabled: true, unavailable_reason: null, max_turns: 30, max_minutes: 15 }),
    start: (req) => {
      scriptSeen = req.script_seen;
      transcript.length = 0;
      transcript.push({ n: 1, speaker: 'customer', text: FIXTURE_OPENING });
      return Promise.resolve({ session_id: 'fixture-1', opening: { n: 1, speaker: 'customer', text: FIXTURE_OPENING }, max_turns: 30 });
    },
    get: (id) =>
      Promise.resolve({ session_id: id, transcript: [...transcript], ended: false, finished: result !== null, max_turns: 30, script_seen: scriptSeen, result }),
    turn: async (_id, req, onDelta): Promise<PracticeTurnResponse> => {
      const repN = transcript.length + 1;
      const repCount = repN / 2;
      const closing = req.text.includes('失礼');
      const text = closing ? FIXTURE_CLOSING : (FIXTURE_REPLIES[repCount - 1] ?? FIXTURE_FALLBACK);
      for (const ch of text) {
        await sleep(delayMs);
        onDelta(ch);
      }
      const rep_turn: TalkTurn = { n: repN, speaker: 'rep', text: req.text };
      const customer_turn: TalkTurn = { n: repN + 1, speaker: 'customer', text };
      transcript.push(rep_turn, customer_turn);
      return { rep_turn, customer_turn, ended: closing };
    },
    finish: (_id, req) => {
      const reps = transcript.filter((t) => t.speaker === 'rep');
      const q = reps.length;
      const labels: Record<string, string> = { hot: '熱い', mixed: 'どちらとも言えない', cold: '冷めている' };
      const guessLabel = labels[req.heat_guess] ?? '熱い';
      const matches = req.heat_guess === 'hot';
      result = {
        items: [
          { item: 'hiring', label: '① 時期と人数', status: 'got', status_label: '取れた', your_record: '3か月以内の職種と人数: ドライバー 2名', truth: '3か月以内の職種と人数: ドライバー 2名', turns: [2] },
          { item: 'issue', label: '② 課題と意欲', status: 'wrong_record', status_label: '記録が違う', your_record: '課題: 空欄 / 解決の早さ: そこまでではない', truth: '課題: 前任が辞めて配車が回らない / 解決の早さ: 早く解決したい', turns: [4] },
          { item: 'cost', label: '③ 検討と予算', status: 'not_heard', status_label: '聞けていないと記録', your_record: '検討: 聞けていない / 予算: 聞けていない', truth: '検討: 前向き / 予算: ある', turns: [] },
          { item: 'decider', label: '④ 決める人と同席', status: 'recorded_without_answer', status_label: '聞かずに記入', your_record: '決める人: 上の人 / 同席: 来る', truth: '決める人: 上の人 / 同席: 来る', turns: [] },
        ],
        not_heard_left: 1,
        not_heard_items: ['③ 検討と予算'],
        question_count: q,
        question_band: fixtureBand(q),
        question_bands: BAND_ROWS,
        shortest_note: '4 項目を聞く最短は 6〜8 問が目安です。',
        monthly_fee_told: false,
        asked_price: false,
        irritations: [],
        heat: {
          guess: req.heat_guess,
          truth: 'hot',
          hot_marks_hidden: 5,
          hot_marks_heard: 2,
          implied_from_heard: 'cold',
          guess_matches_truth: matches,
          message: matches
            ? '見立ては当たりですが、聞き出せた答えからは「冷めている」が妥当でした。隠れた答えまで聞けていれば、根拠をもって判断できました。'
            : `見立て「${guessLabel}」は外れでした。本当の型は「熱い」です。`,
        },
        persona: {
          id: 'hot_01',
          label: '熱い型',
          reference: '導入時期 1〜2 か月以内 × 予算あり × 拠点で決裁 16.5% (それ以外 4.4%、236 件中 39)',
          setting: '運送会社 (架空)の総務課長 (40人ほど)',
          hidden: [
            { label: '3か月以内の職種と人数', value: 'ドライバー 2名' },
            { label: '課題', value: '前任が辞めて配車が回らない' },
            { label: '予算', value: 'ある' },
            { label: '決める人', value: '上の人' },
          ],
        },
        commentary: {
          summary: '検討と予算を聞く前に、月額定額のサービスであることを伝えるとよいです',
          good: [{ turn: 2, text: reps[0]?.text ?? '', comment: '3か月以内と区切って聞けています', better: null }],
          bad: [{ turn: 4, text: reps[1]?.text ?? '', comment: '課題と早さを一度に聞くと、片方の答えしか返ってこないことがあります', better: '今の採用で、いちばん困っていることは何ですか？' }],
        },
        commentary_note: null,
        script_seen: scriptSeen,
        transcript: [...transcript],
        reference_note: '成約率は相関の値です。聞き方を変えたときの成約の増え方は示していません。人・時期をそろえていない素の率です。',
      };
      return Promise.resolve(result);
    },
  };
}
