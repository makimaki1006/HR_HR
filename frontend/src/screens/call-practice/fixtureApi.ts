// ?mode=fixture: サーバもお客さん役も呼ばず、決まった返事で最後まで動かす (画面の確認と E2E 用)。
// 返事は熱い型の架空の人物。結果の数字は、送った発言の数など画面の操作から決まる。

import type { PracticeResult } from '../../generated/PracticeResult';
import type { PracticeTurnResponse } from '../../generated/PracticeTurnResponse';
import type { TalkTurn } from '../../generated/TalkTurn';
import type { AppointmentResult } from '../../generated/AppointmentResult';
import type { PracticeApi } from './api';

export const FIXTURE_OPENING = 'はい、ではその日でお願いします';
export const FIXTURE_PERSON_OPENING = 'はい、お電話代わりました';
export const FIXTURE_OBJECTION = '今は大丈夫ですね';
export const FIXTURE_ACCEPT = 'じゃあ、その日でいいですよ';
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

/** サーバの is_date_offer と同じ考え方: 週・曜日・日・時間帯を出した発言 */
export function isDateOffer(t: string): boolean {
  return /来週|再来週|今週|来月|曜|明日|明後日|午前|午後|[0-9０-９][日時]/.test(t);
}

export function createFixtureApi(delayMs = 15): PracticeApi {
  const transcript: TalkTurn[] = [];
  let scriptSeen = false;
  let range: 'appointment' | 'hearing' | 'full' = 'hearing';
  let agreed = false;
  let ended = false;
  let hearingCount = 0;
  const offerTurns: number[] = [];
  const objectionTurns: number[] = [];
  let agreedTurn: number | null = null;
  const agreedBefore = (reps: TalkTurn[]): number => reps.filter((t) => agreedTurn !== null && t.n < agreedTurn).length;
  let result: PracticeResult | null = null;
  const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

  return {
    config: () => Promise.resolve({ enabled: true, unavailable_reason: null, max_turns: 30, max_minutes: 15 }),
    start: (req) => {
      scriptSeen = req.script_seen;
      range = req.range === 'appointment' || req.range === 'full' ? req.range : 'hearing';
      agreed = range === 'hearing';
      ended = false;
      hearingCount = 0;
      offerTurns.length = 0;
      objectionTurns.length = 0;
      agreedTurn = null;
      result = null;
      const text = range === 'hearing' ? FIXTURE_OPENING : FIXTURE_PERSON_OPENING;
      transcript.length = 0;
      transcript.push({ n: 1, speaker: 'customer', text });
      return Promise.resolve({ session_id: 'fixture-1', range, opening: { n: 1, speaker: 'customer', text }, max_turns: 30 });
    },
    get: (id) =>
      Promise.resolve({ session_id: id, range, date_agreed: agreed, transcript: [...transcript], ended, finished: result !== null, max_turns: 30, script_seen: scriptSeen, result }),
    turn: async (_id, req, onDelta): Promise<PracticeTurnResponse> => {
      const repN = transcript.length + 1;
      const closing = req.text.includes('失礼');
      let text: string;
      let endsNow = closing;
      if (!agreed) {
        if (isDateOffer(req.text)) {
          offerTurns.push(repN);
          if (offerTurns.length >= 2) {
            text = FIXTURE_ACCEPT;
            agreed = true;
            agreedTurn = repN + 1;
            if (range === 'appointment') endsNow = true;
          } else {
            text = FIXTURE_OBJECTION;
            objectionTurns.push(repN + 1);
          }
        } else {
          text = 'はあ、何の件ですか';
        }
      } else {
        text = closing ? FIXTURE_CLOSING : (FIXTURE_REPLIES[hearingCount] ?? FIXTURE_FALLBACK);
        if (!closing) hearingCount += 1;
      }
      ended = endsNow;
      for (const ch of text) {
        await sleep(delayMs);
        onDelta(ch);
      }
      const rep_turn: TalkTurn = { n: repN, speaker: 'rep', text: req.text };
      const customer_turn: TalkTurn = { n: repN + 1, speaker: 'customer', text };
      transcript.push(rep_turn, customer_turn);
      return { rep_turn, customer_turn, ended: endsNow, date_agreed: agreed };
    },
    finish: (_id, req) => {
      const reps = transcript.filter((t) => t.speaker === 'rep');
      const q = reps.length;
      const labels: Record<string, string> = { hot: '熱い', mixed: 'どちらとも言えない', cold: '冷めている' };
      const guessLabel = labels[req.heat_guess] ?? '熱い';
      const matches = req.heat_guess === 'hot';
      const reps0 = transcript.filter((t) => t.speaker === 'rep');
      const appointment: AppointmentResult | null =
        range === 'hearing'
          ? null
          : {
              agreed,
              offers: offerTurns.length,
              offer_turns: [...offerTurns],
              rep_turns_to_agree: agreedTurn === null ? null : agreedBefore(reps0),
              seconds_to_agree: agreedTurn === null ? null : 42,
              objections: objectionTurns.map((c) => {
                const next = transcript.find((t) => t.n > c && t.speaker === 'rep');
                return {
                  turn: c,
                  customer_text: transcript.find((t) => t.n === c)?.text ?? '',
                  next_rep_turn: next?.n ?? null,
                  next_rep_text: next?.text ?? null,
                  reoffered: next !== undefined && offerTurns.includes(next.n),
                };
              }),
              persona_rule: '2 回目の打診で日時を受ける型',
              result_note: agreed
                ? `日時が決まりました。日時の打診は ${String(offerTurns.length)} 回、決まるまでのあなたの発言は ${String(agreedTurn === null ? 0 : agreedBefore(reps0))} 回、42 秒でした。`
                : `日時は決まりませんでした。この型は 2 回目の打診で日時を受けます。あなたの打診は ${String(offerTurns.length)} 回でした。`,
              checks: [
                { id: 'early_date', stage: '日付の前', title: '早く日付を出す', status: 'done', status_label: 'できた', advice: null, quote: reps0[0]?.text ?? null, evidence: [2], baseline_text: '報告書では、話せた電話のうちの 81% が「できていない」' },
                { id: 'kizonkan', stage: '日付の前', title: '既存感(前回のこと・改めた理由・覚えていますか)', status: 'not_done', status_label: 'できていない', advice: '「覚えていただけてますか? 前回お電話したんですけど、タイミングが合わなかったので」と、前回のことと改めた理由をセットで言う。', quote: null, evidence: [], baseline_text: null },
                { id: 'busy_reoffer', stage: '断られた後', title: '「忙しい」「その日は無理」で引かない', status: 'done', status_label: 'できた', advice: null, quote: null, evidence: [], baseline_text: '報告書では、「忙しい」と言われた電話のうちの 11% が「できていない」' },
              ],
              summary: '最初の電話の入りで、前回のことと改めた理由を添えるとよいです。',
              checks_note: null,
            };
      const heatOn = range !== 'appointment';
      result = {
        range,
        appointment,
        items: !heatOn ? [] : [
          { item: 'hiring', label: '① 時期と人数', status: 'got', status_label: '取れた', your_record: '3か月以内の職種と人数: ドライバー 2名', truth: '3か月以内の職種と人数: ドライバー 2名', turns: [2] },
          { item: 'issue', label: '② 課題と意欲', status: 'wrong_record', status_label: '記録が違う', your_record: '課題: 空欄 / 解決の早さ: そこまでではない', truth: '課題: 前任が辞めて配車が回らない / 解決の早さ: 早く解決したい', turns: [4] },
          { item: 'cost', label: '③ 検討と予算', status: 'not_heard', status_label: '聞けていないと記録', your_record: '検討: 聞けていない / 予算: 聞けていない', truth: '検討: 前向き / 予算: ある', turns: [] },
          { item: 'decider', label: '④ 決める人と同席', status: 'recorded_without_answer', status_label: '聞かずに記入', your_record: '決める人: 上の人 / 同席: 来る', truth: '決める人: 上の人 / 同席: 来る', turns: [] },
        ],
        not_heard_left: heatOn ? 1 : 0,
        not_heard_items: heatOn ? ['③ 検討と予算'] : [],
        question_count: q,
        question_band: fixtureBand(q),
        question_bands: heatOn ? BAND_ROWS : [],
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
          hidden: !heatOn ? [] : [
            { label: '3か月以内の職種と人数', value: 'ドライバー 2名' },
            { label: '課題', value: '前任が辞めて配車が回らない' },
            { label: '予算', value: 'ある' },
            { label: '決める人', value: '上の人' },
          ],
        },
        commentary: !heatOn ? null : {
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
