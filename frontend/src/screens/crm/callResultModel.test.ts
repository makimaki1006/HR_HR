import { describe, expect, it } from 'vitest';
import {
  DRAFT_STORAGE_KEY, RESULT_PROPERTY_ALLOWLIST, activeFields, clearDraftEntry, draftKey, editDraft, draftSummary, emptyResultDraft, isCalendarDate,
  loadStore, markRecorded, nextUnrecorded, parseStore, putDraft, saveStore, msUntilNextJstMidnight, toHubSpotPatch, todayJst, validateResultDraft, withField, withOutcome, optionLabel, FIELD_PROPERTY, FALLBACK_LABELS,
} from './callResultModel';
import type { ResultDraft } from './callResultModel';
import { MOC_DEAL_PROPERTIES } from './mocProperties';

const defs = MOC_DEAL_PROPERTIES;
const TODAY = '2026-10-08';
const draft = (over: Partial<ResultDraft>): ResultDraft => ({ ...emptyResultDraft(), ...over });
const errs = (d: Partial<ResultDraft>) => validateResultDraft(draft(d), defs, TODAY);

describe('todayJst / isCalendarDate', () => {
  it('the day changes at JST midnight, not UTC midnight', () => {
    expect(todayJst(Date.UTC(2026, 9, 7, 14, 59, 59))).toBe('2026-10-07'); // JST 23:59:59
    expect(todayJst(Date.UTC(2026, 9, 7, 15, 0, 0))).toBe('2026-10-08'); // JST 00:00
  });
  it('msUntilNextJstMidnight counts down to the next JST 00:00', () => {
    expect(msUntilNextJstMidnight(Date.UTC(2026, 9, 7, 14, 59, 59))).toBe(1000); // JST 23:59:59
    expect(msUntilNextJstMidnight(Date.UTC(2026, 9, 7, 15, 0, 0))).toBe(86_400_000); // JST 00:00 ちょうど → 次の日
    expect(msUntilNextJstMidnight(Date.UTC(2026, 9, 8, 3, 0, 0))).toBe(12 * 3600_000); // JST 12:00
  });
  it('accepts only real calendar dates in YYYY-MM-DD', () => {
    expect(isCalendarDate('2026-10-08')).toBe(true);
    expect(isCalendarDate('2028-02-29')).toBe(true);
    expect(isCalendarDate('2026-02-29')).toBe(false);
    expect(isCalendarDate('2026-9-1')).toBe(false);
    expect(isCalendarDate('2026/10/08')).toBe(false);
  });
});

describe('activeFields', () => {
  const list = (d: Partial<ResultDraft>) => [...activeFields(draft(d))].sort();
  it('only the memo before an outcome is chosen', () => { expect(list({})).toEqual(['memo']); });
  it('connected: partner, next action and next call; interest only after 担当者', () => {
    expect(list({ outcome: 'connected' })).toEqual(['memo', 'nextAction', 'nextCallDate', 'nextCallTime', 'spokeTo']);
    expect(list({ outcome: 'connected', spokeTo: '担当者' })).toContain('interest');
    expect(list({ outcome: 'connected', spokeTo: '受付' })).not.toContain('interest');
  });
  it('wrong_number: unreachable check (+ its other reason), no next call, no partner', () => {
    expect(list({ outcome: 'wrong_number' })).toEqual(['memo', 'unreachable']);
    expect(list({ outcome: 'wrong_number', unreachable: 'その他' })).toEqual(['memo', 'unreachable', 'unreachableOther']);
  });
  it('do_not_call: stop / block reason, never the next call', () => {
    expect(list({ outcome: 'do_not_call' })).toEqual(['blockReason', 'memo', 'spokeTo', 'stopReason']);
  });
  it('appointment adds the three appointment fields', () => {
    expect(list({ outcome: 'appointment' })).toEqual(['apptDate', 'apptMethod', 'apptTime', 'memo', 'nextAction', 'nextCallDate', 'nextCallTime', 'spokeTo']);
  });
  it('no_answer: unreachable check and next call, no partner', () => {
    expect(list({ outcome: 'no_answer' })).toEqual(['memo', 'nextAction', 'nextCallDate', 'nextCallTime', 'unreachable']);
  });
});

describe('withOutcome', () => {
  it('callback pre-selects 再架電 only when the next action is empty', () => {
    expect(withOutcome(draft({}), 'callback').nextAction).toBe('再架電');
    expect(withOutcome(draft({ nextAction: '資料送付' }), 'callback').nextAction).toBe('資料送付');
    expect(withOutcome(draft({}), 'connected').nextAction).toBe('');
  });
  it('an auto-filled 再架電 is removed when the outcome moves away from callback (no required next call the caller never chose)', () => {
    for (const to of ['appointment', 'connected', 'no_answer'] as const) {
      const d = withOutcome(withOutcome(draft({}), 'callback'), to);
      expect(d.nextAction).toBe('');
      expect(d.nextActionAuto).toBe(false);
      expect(errs(d)).not.toHaveProperty('nextCallDate');
      expect(errs(d)).not.toHaveProperty('nextCallTime');
    }
    // アポに切り替えたら必須は doc §2 の 3 項目だけ、送る内容に bpo_45 は入らない
    const appt = withOutcome(withOutcome(draft({}), 'callback'), 'appointment');
    expect(errs(appt)).toEqual({ apptDate: '商談予定日を入れてください。', apptTime: '商談予定時間を選んでください。', apptMethod: '商談方法を選んでください。' });
    expect(toHubSpotPatch({ ...appt, apptDate: '2026-10-20', apptTime: '9:30', apptMethod: 'zoom' }, defs, TODAY))
      .toEqual({ properties: { bpo_23: '2026-10-20', bpo__: '9:30', bpo_33: 'zoom' } });
    // 戻すとまた自動で入る
    expect(withOutcome(appt, 'callback').nextAction).toBe('再架電');
  });
  it('a 再架電 the caller picked themselves is kept when the outcome changes', () => {
    const picked = withField(withOutcome(draft({}), 'callback'), 'nextAction', '再架電');
    expect(picked.nextActionAuto).toBe(false);
    expect(withOutcome(picked, 'connected').nextAction).toBe('再架電');
    const chosenFirst = withOutcome(withField(withOutcome(draft({}), 'connected'), 'nextAction', '再架電'), 'callback');
    expect(withOutcome(chosenFirst, 'no_answer').nextAction).toBe('再架電');
    // 自動で入った後に別の値へ変えたら、その値は残る
    expect(withOutcome(withField(withOutcome(draft({}), 'callback'), 'nextAction', '資料送付'), 'connected').nextAction).toBe('資料送付');
  });
});

describe('optionLabel', () => {
  it('maps stored values to HubSpot display labels, including hidden options, falling back to the value', () => {
    expect(optionLabel(defs.bpo_10, '使われておりません')).toBe('現在使われておりません');
    expect(optionLabel(defs.bpo_4, 'リスト被り')).toBe('リスト被り（架電被り）');
    expect(optionLabel(defs.bpo_10, '未知の値')).toBe('未知の値');
    expect(optionLabel(undefined, 'x')).toBe('x');
  });
});

describe('validateResultDraft', () => {
  it('requires the outcome', () => { expect(errs({})).toEqual({ outcome: '今回の結果を選んでください。' }); });
  it('a plain connected call with nothing else is valid', () => { expect(errs({ outcome: 'connected' })).toEqual({}); });
  it('callback requires both next call date and time', () => {
    expect(errs({ outcome: 'callback', nextAction: '再架電' })).toEqual({
      nextCallDate: '次回架電日を入れてください(再架電のとき必須)。', nextCallTime: '次回架電時間を選んでください(再架電のとき必須)。',
    });
    expect(errs({ outcome: 'callback', nextAction: '再架電', nextCallDate: '2026-10-09', nextCallTime: '9:15' })).toEqual({});
  });
  it('next action 再架電 on a connected call also requires date and time', () => {
    expect(Object.keys(errs({ outcome: 'connected', nextAction: '再架電' })).sort()).toEqual(['nextCallDate', 'nextCallTime']);
  });
  it('date and time go together even when optional', () => {
    expect(errs({ outcome: 'connected', nextCallDate: '2026-10-09' })).toEqual({ nextCallTime: '次回架電時間も選んでください(日付とセット)。' });
    expect(errs({ outcome: 'no_answer', nextCallTime: '10:00' })).toEqual({ nextCallDate: '次回架電日も入れてください(時間とセット)。' });
  });
  it('dates must be real and not before today (JST)', () => {
    expect(errs({ outcome: 'connected', nextCallDate: '2026-10-07', nextCallTime: '9:00' })).toEqual({ nextCallDate: '今日以降の日付を入れてください。' });
    expect(errs({ outcome: 'connected', nextCallDate: '2026-10-08', nextCallTime: '9:00' })).toEqual({});
    expect(errs({ outcome: 'connected', nextCallDate: '2026-02-30', nextCallTime: '9:00' })).toEqual({ nextCallDate: '日付を確認してください。' });
  });
  it('time must be one of the option values (15-minute steps); 9:10 is not an option', () => {
    expect(errs({ outcome: 'connected', nextCallDate: '2026-10-09', nextCallTime: '9:10' })).toEqual({ nextCallTime: '選択肢にない値です。選び直してください。' });
  });
  it('appointment requires date, time (30-minute options) and method', () => {
    expect(errs({ outcome: 'appointment' })).toEqual({
      apptDate: '商談予定日を入れてください。', apptTime: '商談予定時間を選んでください。', apptMethod: '商談方法を選んでください。',
    });
    expect(errs({ outcome: 'appointment', apptDate: '2026-10-20', apptTime: '9:15', apptMethod: 'zoom' })).toEqual({ apptTime: '選択肢にない値です。選び直してください。' });
    expect(errs({ outcome: 'appointment', apptDate: '2026-10-20', apptTime: '9:30', apptMethod: 'zoom' })).toEqual({});
  });
  it('do_not_call requires the stop reason (max 200 chars) and ignores a leftover next call', () => {
    expect(errs({ outcome: 'do_not_call' })).toEqual({ stopReason: '架電禁止理由を入れてください。' });
    expect(errs({ outcome: 'do_not_call', stopReason: 'あ'.repeat(201) })).toEqual({ stopReason: '200 文字以内で入力してください(いま 201 文字)。' });
    expect(errs({ outcome: 'do_not_call', stopReason: '先方の希望', nextAction: '再架電', nextCallDate: '2020-01-01' })).toEqual({});
  });
  it('wrong_number requires the unreachable check; その他 requires the reason', () => {
    expect(errs({ outcome: 'wrong_number' })).toEqual({ unreachable: '不通時チェックを選んでください。' });
    expect(errs({ outcome: 'wrong_number', unreachable: 'その他' })).toEqual({ unreachableOther: '「その他」の理由を入れてください。' });
    expect(errs({ outcome: 'wrong_number', unreachable: '使われておりません' })).toEqual({});
  });
  it('the display label is not accepted as a value (現在使われておりません → 使われておりません)', () => {
    expect(errs({ outcome: 'wrong_number', unreachable: '現在使われておりません' })).toEqual({ unreachable: '選択肢にない値です。選び直してください。' });
  });
  it('a hidden option cannot be chosen anew', () => {
    const hidden = { ...defs, bpo_40: { name: "bpo_40", label: "接触結果", type: "enumeration", fieldType: "select", editable: true, options: [{ label: '受付', value: '受付', hidden: true }, { label: '担当者', value: '担当者', hidden: false }] } };
    expect(validateResultDraft(draft({ outcome: 'connected', spokeTo: '受付' }), hidden, TODAY)).toEqual({ spokeTo: '選択肢にない値です。選び直してください。' });
  });
  it('a date / text field whose HubSpot type changed is refused (the future write would send a wrong-shaped value)', () => {
    const d13 = defs.bpo_13; const d16 = defs.bpo_16;
    if (!d13 || !d16) throw new Error('snapshot lacks bpo_13 / bpo_16');
    const changed = { ...defs, bpo_13: { ...d13, type: 'string' }, bpo_16: { ...d16, type: 'number' } };
    expect(validateResultDraft(draft({ outcome: 'connected', nextCallDate: '2026-10-09', nextCallTime: '9:15', memo: 'm' }), changed, TODAY)).toEqual({
      nextCallDate: 'HubSpot 側でこの項目の設定が変わったため、ここでは入力できません。管理者に連絡してください。',
      memo: 'HubSpot 側でこの項目の設定が変わったため、ここでは入力できません。管理者に連絡してください。',
    });
    expect(toHubSpotPatch(draft({ outcome: 'connected', memo: 'm' }), changed, TODAY)).toBeNull();
    // 空の項目は型を問わない
    expect(validateResultDraft(draft({ outcome: 'connected' }), changed, TODAY)).toEqual({});
  });
  it('a required text field that holds only spaces counts as empty', () => {
    expect(errs({ outcome: 'do_not_call', stopReason: '   ' })).toEqual({ stopReason: '架電禁止理由を入れてください。' });
    expect(errs({ outcome: 'do_not_call', stopReason: '\u3000\n' })).toEqual({ stopReason: '架電禁止理由を入れてください。' });
    expect(errs({ outcome: 'wrong_number', unreachable: 'その他', unreachableOther: ' ' })).toEqual({ unreachableOther: '「その他」の理由を入れてください。' });
  });
  it('the その他 reason is limited to 200 characters', () => {
    expect(errs({ outcome: 'wrong_number', unreachable: 'その他', unreachableOther: 'あ'.repeat(200) })).toEqual({});
    expect(errs({ outcome: 'wrong_number', unreachable: 'その他', unreachableOther: 'あ'.repeat(201) }))
      .toEqual({ unreachableOther: '200 文字以内で入力してください(いま 201 文字)。' });
  });
  it('memo is limited to 2000 characters', () => {
    expect(errs({ outcome: 'connected', memo: 'x'.repeat(2000) })).toEqual({});
    expect(errs({ outcome: 'connected', memo: 'x'.repeat(2001) })).toEqual({ memo: '2000 文字以内で入力してください(いま 2001 文字)。' });
  });
});

describe('toHubSpotPatch', () => {
  it('callback: internal names, YYYY-MM-DD date, the time option value, only non-empty fields', () => {
    expect(toHubSpotPatch(draft({
      outcome: 'callback', spokeTo: '担当者', interest: '中（検討余地あり）', nextAction: '再架電', nextCallDate: '2026-10-09', nextCallTime: '9:15', memo: '午後に再度\n',
    }), defs, TODAY)).toEqual({ properties: {
      bpo_40: '担当者', bpo_42: '中（検討余地あり）', bpo_45: '再架電', bpo_13: '2026-10-09', bpo_14: '9:15', bpo_16: '午後に再度',
    } });
  });
  it('fields hidden for the outcome are never sent, even if they still hold a value', () => {
    const d = draft({ outcome: 'do_not_call', stopReason: ' 先方の希望 ', blockReason: 'クレーム懸念案件', nextAction: '再架電', nextCallDate: '2026-10-09', nextCallTime: '9:15', apptDate: '2026-10-20', interest: '高（前向き）' });
    expect(toHubSpotPatch(d, defs, TODAY)).toEqual({ properties: { bpo_3: '先方の希望', bpo_4: 'クレーム懸念案件' } });
  });
  it('the memo keeps its leading indentation and line breaks; only trailing spaces are removed', () => {
    expect(toHubSpotPatch(draft({ outcome: 'connected', memo: '  1行目\n2行目  \n' }), defs, TODAY)).toEqual({ properties: { bpo_16: '  1行目\n2行目' } });
    expect(toHubSpotPatch(draft({ outcome: 'connected', memo: '\n先頭が改行' }), defs, TODAY)).toEqual({ properties: { bpo_16: '\n先頭が改行' } });
  });
  it('wrong_number with その他 sends the stored value その他 and the reason', () => {
    expect(toHubSpotPatch(draft({ outcome: 'wrong_number', unreachable: 'その他', unreachableOther: '法人番号の誤り' }), defs, TODAY))
      .toEqual({ properties: { bpo_10: 'その他', bpo_57: '法人番号の誤り' } });
  });
  it('appointment uses the 30-minute time value and method value', () => {
    expect(toHubSpotPatch(draft({ outcome: 'appointment', spokeTo: '担当者', apptDate: '2026-10-20', apptTime: '14:30', apptMethod: 'zoom+電話' }), defs, TODAY))
      .toEqual({ properties: { bpo_40: '担当者', bpo_23: '2026-10-20', bpo__: '14:30', bpo_33: 'zoom+電話' } });
  });
  it('an outcome alone sends nothing (outcome is UI-only, not a HubSpot property); invalid drafts give null', () => {
    expect(toHubSpotPatch(draft({ outcome: 'no_answer' }), defs, TODAY)).toEqual({ properties: {} });
    expect(toHubSpotPatch(draft({ outcome: 'callback' }), defs, TODAY)).toBeNull();
    expect(toHubSpotPatch(draft({}), defs, TODAY)).toBeNull();
  });
  it('appointment with everything filled: exactly these 9 properties and values, never dealstage / bpo_20 / outcome', () => {
    const p = toHubSpotPatch(draft({ outcome: 'appointment', spokeTo: '担当者', interest: '高（前向き）', nextAction: '資料送付', nextCallDate: '2026-10-09', nextCallTime: '8:00',
      memo: 'm', apptDate: '2026-10-20', apptTime: '8:00', apptMethod: '電話' }), defs, TODAY);
    expect(p).toEqual({ properties: {
      bpo_40: '担当者', bpo_42: '高（前向き）', bpo_45: '資料送付', bpo_13: '2026-10-09', bpo_14: '8:00', bpo_16: 'm', bpo_23: '2026-10-20', bpo__: '8:00', bpo_33: '電話',
    } });
  });
  it('the allowlist is its own literal list: every FIELD_PROPERTY name is in it, and nothing else', () => {
    expect([...RESULT_PROPERTY_ALLOWLIST].sort()).toEqual(['bpo_10', 'bpo_13', 'bpo_14', 'bpo_16', 'bpo_23', 'bpo_3', 'bpo_33', 'bpo_4', 'bpo_40', 'bpo_42', 'bpo_45', 'bpo_57', 'bpo__']);
    expect([...Object.values(FIELD_PROPERTY)].sort()).toEqual([...RESULT_PROPERTY_ALLOWLIST].sort());
    expect(RESULT_PROPERTY_ALLOWLIST).not.toContain('dealstage');
    expect(RESULT_PROPERTY_ALLOWLIST).not.toContain('bpo_20');
  });
  it('every field has a Japanese fallback heading (internal names never become headings)', () => {
    for (const name of Object.values(FIELD_PROPERTY)) expect(FALLBACK_LABELS[name]).toMatch(/[\u3000-\u9fff]/);
  });
});

describe('draftSummary', () => {
  it('one line with labels, next call and memo flag', () => {
    expect(draftSummary(draft({}), defs)).toBe('結果は未選択');
    expect(draftSummary(draft({ outcome: 'callback', spokeTo: '担当者', nextAction: '再架電', nextCallDate: '2026-10-09', nextCallTime: '9:15', memo: 'x' }), defs))
      .toBe('再架電の約束 · 担当者 · 次回 10/09 9:15 · メモあり');
  });
});

describe('draft store', () => {
  it('keys include the mode so live and fixture ids never mix', () => {
    expect(draftKey('live', '1')).toBe('live:1');
    expect(draftKey('fixture', '1')).not.toBe(draftKey('live', '1'));
  });
  it('put / mark / clear', () => {
    let s = putDraft({ drafts: {}, recorded: {} }, 'live:1', draft({ outcome: 'connected' }));
    s = markRecorded(s, 'live:1');
    expect(s).toEqual({ drafts: { 'live:1': draft({ outcome: 'connected' }) }, recorded: { 'live:1': true } });
    expect(putDraft(s, 'live:1', draft({})).drafts).toEqual({});
    expect(clearDraftEntry(s, 'live:1')).toEqual({ drafts: {}, recorded: {} });
  });
  it('editing a recorded draft removes its recorded mark (only for that key)', () => {
    let s = putDraft({ drafts: {}, recorded: {} }, 'live:1', draft({ outcome: 'connected' }));
    s = markRecorded(markRecorded(s, 'live:1'), 'live:2');
    const edited = editDraft(s, 'live:1', draft({ outcome: 'appointment' }));
    expect(edited).toEqual({ drafts: { 'live:1': draft({ outcome: 'appointment' }) }, recorded: { 'live:2': true } });
    expect(editDraft(s, 'live:1', draft({}))).toEqual({ drafts: {}, recorded: { 'live:2': true } });
  });
  it('parseStore keeps valid drafts and drops malformed ones', () => {
    const raw = JSON.stringify({
      drafts: { 'live:1': { outcome: 'callback', memo: 'm' }, 'live:2': { outcome: 'bogus' }, 'live:3': { memo: 5 }, 'live:4': 'x' },
      recorded: { 'live:1': true, 'live:2': 'yes' },
    });
    expect(parseStore(raw)).toEqual({ drafts: { 'live:1': draft({ outcome: 'callback', memo: 'm' }) }, recorded: { 'live:1': true } });
    expect(parseStore('{not json')).toEqual({ drafts: {}, recorded: {} });
    // 自動で入れた印 (真偽値) は戻す。真偽値でなければその下書きを捨てる
    expect(parseStore(JSON.stringify({ drafts: { 'live:5': { outcome: 'callback', nextAction: '再架電', nextActionAuto: true }, 'live:6': { nextActionAuto: 'yes' } } })).drafts)
      .toEqual({ 'live:5': draft({ outcome: 'callback', nextAction: '再架電', nextActionAuto: true }) });
    expect(parseStore(null)).toEqual({ drafts: {}, recorded: {} });
  });
  it('load / save survive a storage that throws', () => {
    const throwing = { getItem: () => { throw new Error('denied'); }, setItem: () => { throw new Error('quota'); } };
    expect(loadStore(throwing)).toEqual({ drafts: {}, recorded: {} });
    expect(saveStore(throwing, { drafts: {}, recorded: {} })).toBe(false);
    expect(saveStore(null, { drafts: {}, recorded: {} })).toBe(false);
    const mem = new Map<string, string>();
    expect(saveStore({ setItem: (k, v) => { mem.set(k, v); } }, { drafts: { 'live:9': draft({ memo: 'a' }) }, recorded: {} })).toBe(true);
    expect(loadStore({ getItem: k => mem.get(k) ?? null }).drafts['live:9']?.memo).toBe('a');
    expect([...mem.keys()]).toEqual([DRAFT_STORAGE_KEY]);
  });
});

describe('nextUnrecorded', () => {
  const rec = (ids: string[]) => (id: string) => ids.includes(id);
  it('the next unrecorded row after the current one, wrapping to the top', () => {
    expect(nextUnrecorded(['1', '2', '3', '4'], '1', rec(['2']))).toBe('3');
    expect(nextUnrecorded(['1', '2', '3', '4'], '4', rec([]))).toBe('1');
    expect(nextUnrecorded(['1', '2', '3'], '2', rec(['1', '3']))).toBeNull();
    expect(nextUnrecorded(['1', '2'], 'gone', rec(['1']))).toBe('2');
  });
});
