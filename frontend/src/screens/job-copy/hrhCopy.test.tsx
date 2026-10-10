// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { hrhCopySections, composeHrhBody, composeSalary } from './hrhCopy';
import { JobCopyBody } from './JobCopyBody';
import { extractSalary, salaryLabel } from './salaryExtract';
import { listingRecord } from './hubspotListings';
import { versionChanges, changeKinds } from './timelineModel';
import { compareCopy } from './diff';
import type { HubSpotVersions } from './hubspotListings';
afterEach(cleanup);
const body = '案件名：架空の配送求人\n仕事内容：\n日用品を届けます。\n例：朝に積み込みます。\nキャッチコピー：地域を支える仕事\n仕事情報補足1のタイトル：入社後の流れ\n仕事情報補足2のタイトル：職場の雰囲気\n仕事情報補足1の内容：先輩と同行します。\n仕事情報補足2の内容：チームで相談できます。\nIndeed表示職種名：ドライバー\n給与形態：月給\n基本給与 最小：250000\n基本給与 最大：280000\nタスクの所要時間：2\nタスクの単位：時間\n平均稼働時間：1日8時間\n平均稼働日数：月20日\n固定残業代：30000\n想定残業時間：月20時間\n条件付き給与1 条件：夜間勤務の場合\n条件付き給与1 深夜帯：22時〜5時\n条件付き給与1 最小給与：270000\n条件付き給与1 最大給与：300000\n給与補足：交通費支給\n試用・研修の有無：あり\n試用・研修時の雇用条件：給与が異なります\n試用・研修期の給与のタイプ：月給\n試用・研修期の基本給与 最小：240000\n試用・研修期の基本給与 最大：240000\n試用・研修期の平均稼働時間：1日8時間\n試用・研修の詳細情報：同乗研修2週間\n自由項目1のタイトル：休日・休暇\n自由項目1の内容：週休2日\n自由項目2の内容：制服貸与';
const history: HubSpotVersions = { listing: { id: '42', media: 'hrh', media_job_id: 'SAMPLE42', account_id: null, title: '架空求人', prefecture: null, municipality: null, category: null, publication_status: null, last_csv_detected_at: null, application_count: null }, versions: [{ written_at: '2026-10-01T00:00:00Z', body, image_urls: [] }, { written_at: '2026-10-02T00:00:00Z', body: body.replace('基本給与 最小：250000', '基本給与 最小：260000'), image_urls: [] }], history_counts: {}, history_may_be_incomplete: false };
describe('本番形式のHRハッカー求人票', () => {
  it('keeps column-like lines inside a three-line description and the actual qualification', () => {
    const sections = hrhCopySections('仕事内容：\nご案内\n応募資格：未経験可\n補足\n応募資格：普通免許');
    expect(sections).toContainEqual({ heading: '仕事内容', text: 'ご案内\n応募資格：未経験可\n補足' });
    expect(sections).toContainEqual({ heading: '応募資格', text: '普通免許' });
  });
  it('pairs separated titles and contents, retains multiline descriptions, and uses applicant-facing headings', () => {
    const sections = hrhCopySections(body);
    expect(sections).toContainEqual({ heading: '仕事内容', text: '日用品を届けます。\n例：朝に積み込みます。' });
    expect(sections).toContainEqual({ heading: '入社後の流れ', text: '先輩と同行します。' });
    expect(sections).toContainEqual({ heading: '職場の雰囲気', text: 'チームで相談できます。' });
    expect(sections).toContainEqual({ heading: '休日・休暇', text: '週休2日' });
    expect(sections).toContainEqual({ heading: 'その他', text: '制服貸与' });
    expect(sections).toContainEqual({ heading: '職種', text: 'ドライバー' });
    render(<JobCopyBody body={composeHrhBody(body)} sections={sections} />);
    expect(screen.getByRole('heading', { name: '入社後の流れ' })).toBeTruthy();
    expect(screen.getByText('月給 250,000円〜280,000円')).toBeTruthy();
    expect(screen.getByLabelText('求人票').textContent).not.toMatch(/仕事情報補足|自由項目|Indeed表示職種名|基本給与|給与形態|平均稼働|タスクの/);
  });
  it('combines salary notes and all trial conditions into readable sections', () => {
    const sections = hrhCopySections(body);
    const notes = sections.find(section => section.heading === '給与・勤務の補足');
    expect(notes?.text).toContain('1回の仕事の時間：2 時間');
    expect(notes?.text).toContain('平均の勤務時間：1日8時間');
    expect(notes?.text).toContain('固定残業代：30,000円');
    expect(notes?.text).toContain('想定される残業時間：月20時間');
    expect(notes?.text).toContain('夜間勤務の場合 ／ 深夜の勤務：22時〜5時 ／ 月給 270,000円〜300,000円');
    expect(notes?.text).toContain('交通費支給');
    const trial = sections.filter(section => section.heading === '試用・研修');
    expect(trial).toHaveLength(1);
    expect(trial[0]?.text).toContain('研修中の給与：月給 240,000円');
    expect(trial[0]?.text).toContain('平均の勤務時間：1日8時間');
    expect(trial[0]?.text).toContain('同乗研修2週間');
  });
  it('extracts the composed base salary from both raw CSV body and comparison body', () => {
    expect(extractSalary(body)).toMatchObject({ kind: '月給', min: 250000, max: 280000 });
    expect(extractSalary(composeHrhBody(body))).toEqual(extractSalary(body));
    expect(composeSalary('月給', '250000', '')).toBe('月給 250,000円〜');
    const lowerOnly = extractSalary('給与形態：月給\n基本給与 最小：250000');
    expect(lowerOnly).toMatchObject({ min: 250000, max: null });
    expect(salaryLabel(lowerOnly)).toBe('月給25万円〜');
    expect(extractSalary(`給与：${composeSalary('月給', '', '280000')}`)).toMatchObject({ min: null, max: 280000 });
    expect(composeSalary('時給', '1200', '1500')).toBe('時給 1,200円〜1,500円');
  });
  it('detects a base minimum increase as salary, with the same amounts in the version comparison', () => {
    const record = listingRecord(history);
    const changes = versionChanges(record);
    expect(changes[1]).toMatchObject({ salaryChanged: true, salaryDirection: 'up', otherBodyChanged: false, salary: { min: 260000, max: 280000 } });
    if (!changes[1]) throw new Error('second change missing');
    expect(changeKinds(changes[1])).toEqual(['給与']);
    const diff = compareCopy(record.versions[0]?.body ?? null, record.versions[1]?.body ?? null);
    expect(diff.lines.filter(line => line.kind !== 'same').map(line => [line.kind, line.text])).toEqual([['removed', '給与：月給 250,000円〜280,000円'], ['added', '給与：月給 260,000円〜280,000円']]);
    expect(record.versions[1]?.bodySections).toContainEqual({ heading: '給与', text: '月給 260,000円〜280,000円' });
  });
  it('keeps AirWork descriptions, including colons, in a single description section', () => {
    const raw = '仕事内容：利用者を支えます。\n給与形態：月給（仕事内容に記載された説明）';
    const record = listingRecord({ ...history, listing: { ...history.listing, media: 'airwork' }, versions: [{ written_at: '2026-10-01T00:00:00Z', body: raw, image_urls: null }] });
    expect(record.versions[0]?.body).toBe(raw);
    expect(record.versions[0]?.bodySections).toEqual([{ heading: '仕事内容', text: raw }]);
  });
  it('does not invent salary or body when the underlying fields are absent', () => {
    expect(hrhCopySections('')).toEqual([]);
    expect(composeHrhBody('')).toBe('');
    expect(extractSalary(composeHrhBody('試用・研修の有無：あり\n試用・研修期の給与のタイプ：月給\n試用・研修期の基本給与 最小：240000'))).toBeNull();
    expect(hrhCopySections('仕事内容：本文のみ')).toEqual([{ heading: '仕事内容', text: '本文のみ' }]);
  });
});
