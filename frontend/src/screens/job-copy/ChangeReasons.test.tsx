// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import { ChangeReasons } from './ChangeReasons';
import { ApplicantReasonReview } from './ApplicantReasonReview';

vi.mock('../../api/client', () => ({ apiGet: vi.fn(), apiPut: vi.fn() }));
afterEach(() => { cleanup(); });

const demo = (id: string): JobCopyRecord => {
  const job = jobs.find(item => item.id === id);
  if (!job) throw new Error(`Missing ${id}`);
  return job;
};

function at<T>(list: readonly T[], index: number): T {
  const found = list[index];
  if (found === undefined) throw new Error(`Missing item ${String(index)}`);
  return found;
}

describe('ChangeReasons', () => {
  it('always shows the caution line and says there is no change when none was found', () => {
    render(<ChangeReasons job={demo('demo-job-008')} onOpenFactors={() => undefined} />);
    expect(screen.getByText('応募理由の記録はまだ少なく、1 つの求人・期間の件数だけで変更の効果は判断できません。')).toBeTruthy();
    expect(screen.getByText('HubSpot に記録された分だけを数えています。')).toBeTruthy();
    expect(screen.getByText('この求人で給与・本文・画像の変更は見つかっていません')).toBeTruthy();
    expect(screen.queryAllByTestId('change-reason-block')).toHaveLength(0);
  });

  it('shows one block per change, newest first, with counts, the left-out records and no percentage', () => {
    const { container } = render(<ChangeReasons job={demo('demo-job-001')} onOpenFactors={() => undefined} />);
    const blocks = screen.getAllByTestId('change-reason-block');
    expect(blocks).toHaveLength(2);
    expect(blocks[0]?.getAttribute('aria-label')).toBe('9/25 給与・本文・画像の変更');
    expect(blocks[1]?.getAttribute('aria-label')).toBe('9/15 給与・本文・画像の変更');
    expect(screen.getByText('応募日が分からない記録 1件は、どの期間にも入れていません。')).toBeTruthy();
    const before = within(at(blocks, 0)).getByRole('group', { name: '変更前 10日' });
    expect(within(before).getByText('応募 8件')).toBeTruthy();
    expect(within(before).getByText('応募理由の記録 5件')).toBeTruthy();
    expect(within(before).getByText('前の変更（9/15）までの10日で区切っています')).toBeTruthy();
    const after = within(at(blocks, 0)).getByRole('group', { name: '変更後 11日' });
    expect(within(after).getByText('応募 3件')).toBeTruthy();
    expect(within(after).getByText('この期間に応募理由の記録はありません')).toBeTruthy();
    expect(container.textContent).not.toContain('%');
    expect(container.textContent).not.toContain('％');
  });

  it('changes the counts when the weeks change', () => {
    const base = demo('demo-job-001');
    render(<ChangeReasons job={base} onOpenFactors={() => undefined} />);
    const earlier = () => at(screen.getAllByTestId('change-reason-block'), 1);
    // 4 weeks: the 14 days of the first version are shorter than 4 weeks.
    expect(within(earlier()).getByText('掲載の記録の始まり（9/01）からの14日で区切っています')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '前後 2週' }));
    expect(within(earlier()).queryByText(/掲載の記録の始まり/u)).toBeNull();
    expect(within(earlier()).getByRole('group', { name: '変更前 14日' })).toBeTruthy();
    expect(screen.getByRole('button', { name: '前後 2週' }).getAttribute('aria-pressed')).toBe('true');
  });

  it('shows the range date of an AirWork change and the reasons of the days that can be counted', () => {
    render(<ChangeReasons job={demo('demo-job-004')} onOpenFactors={() => undefined} />);
    const block = screen.getByTestId('change-reason-block');
    expect(within(block).getByRole('heading', { level: 3 }).textContent).toContain('9/10〜9/28 の間');
    expect(within(block).getByText(/その間の日は、前後どちらの応募にも数えていません/u)).toBeTruthy();
    const after = within(block).getByRole('group', { name: '変更後 5日' });
    expect(within(after).getByText('応募 2件')).toBeTruthy();
    expect(within(after).getByText('応募理由の記録 2件')).toBeTruthy();
    expect(within(block).getByText('この本文が出ていたと確かな日がないため、数えていません')).toBeTruthy();
  });

  it('masks personal details in the texts and offers the factor memo link', () => {
    const base = demo('demo-job-004');
    const reasons = base.applicantReasons;
    if (!reasons) throw new Error('Missing reasons');
    const job: JobCopyRecord = { ...base, applicantReasons: { ...reasons, items: reasons.items.map(row => row.applicationDate === '2026-09-29' ? { ...row, text: '連絡は 090-1234-5678 まで。時給が高いので' } : row) } };
    const open = vi.fn();
    render(<ChangeReasons job={job} onOpenFactors={open} />);
    const text = screen.getByRole('group', { name: '変更後 5日' }).textContent;
    expect(text).toContain('＊＊');
    expect(text).not.toContain('090-1234-5678');
    expect(screen.getAllByText('応募者が書いた文を開く（社内確認用）').length).toBeGreaterThan(0);
    expect(screen.getByText('この求人の要因メモはまだありません')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: '要因メモを開く' }));
    expect(open).toHaveBeenCalledTimes(1);
  });
});

describe('ApplicantReasonReview display switch', () => {
  it('keeps the comparison as the default and switches to the per-change view', () => {
    const change = vi.fn();
    const { rerender } = render(<ApplicantReasonReview job={demo('demo-job-001')} onViewChange={change} />);
    expect(screen.getByRole('button', { name: '記述の比較' }).getAttribute('aria-pressed')).toBe('true');
    expect(screen.queryByLabelText('変更ごとの応募理由')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '変更ごと' }));
    expect(change).toHaveBeenCalledWith('changes');
    rerender(<ApplicantReasonReview job={demo('demo-job-001')} view="changes" onViewChange={change} />);
    expect(screen.getByLabelText('変更ごとの応募理由')).toBeTruthy();
  });
});
