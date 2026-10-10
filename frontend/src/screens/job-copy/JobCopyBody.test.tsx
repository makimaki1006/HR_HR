// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { JobCopyBody, jobCopySections } from './JobCopyBody';
import { extractSalary } from './salaryExtract';
import { listingRecord } from './hubspotListings';
afterEach(cleanup);
describe('求人票の読み取り', () => {
  it('splits labelled fields and keeps multiline values and salary intact', () => {
    const body = '案件名：配送求人\n仕事内容：日用品を届けます。\n同乗研修があります。\n給与：月給28万円〜32万円\n勤務時間：8:00〜17:00\n関連リンク：https://example.invalid/job';
    expect(jobCopySections(body)).toEqual([{ heading: '案件名', text: '配送求人' }, { heading: '仕事内容', text: '日用品を届けます。\n同乗研修があります。' }, { heading: '給与', text: '月給28万円〜32万円' }, { heading: '勤務時間', text: '8:00〜17:00' }, { heading: '関連リンク', text: 'https://example.invalid/job' }]);
    render(<JobCopyBody body={body} />);
    expect(screen.getByRole('heading', { name: '給与' })).toBeTruthy();
    expect(screen.getByText('月給28万円〜32万円')).toBeTruthy();
    expect(extractSalary(body)).toMatchObject({ kind: '月給', min: 280000, max: 320000 });
  });
  it('preserves unlabelled text and explicitly explains an empty body', () => {
    expect(jobCopySections('チームで利用者を支えます。\n9:00から勤務。\nhttps://example.invalid/info')).toEqual([{ heading: '仕事内容', text: 'チームで利用者を支えます。\n9:00から勤務。\nhttps://example.invalid/info' }]);
    render(<JobCopyBody body="" />);
    expect(screen.getByRole('status').textContent).toContain('本文は未取得');
  });
  it('uses a separately labelled current value when history is unavailable', () => {
    const record = listingRecord({ listing: { id: '42', media: 'airwork', media_job_id: 'AW-42', account_id: null, title: '合成求人', prefecture: null, municipality: null, category: null, publication_status: null, last_csv_detected_at: null, application_count: null }, history_counts: { shigotonaiyou: 0 }, history_may_be_incomplete: false, versions: [], current: { body: '現在の仕事内容', checked_at: '2026-10-10T00:00:00Z', image_urls: null }, current_images: { observed_at: '2026-10-09T00:00:00Z', image_urls: ['/api/job-copy/image?sample=1'] } });
    expect(record.versions[0]?.label).toBe('現在の文面（履歴未取得）');
    expect(record.versions[0]?.body).toBe('現在の仕事内容');
    expect(record.versions[0]?.note).toContain('文面の保存日時と掲載開始日時は不明');
    expect(record.currentImageObservation?.observedAt).toBe('2026-10-09T00:00:00Z');
  });
});
