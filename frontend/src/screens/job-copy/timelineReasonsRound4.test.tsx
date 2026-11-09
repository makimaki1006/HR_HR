// @vitest-environment happy-dom
/**
 * Review round 4 of the application reasons: a 未取得 選択済み count never shows as 0件, the lane
 * does not say 「記録はありません」 when the records were only left out, and a section whose texts
 * were cut by the read limit does not say there is nothing to classify.
 */
import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import type { ApplicantReasonCollection } from './applicantReasonsModel';
import { JobTimeline } from './JobTimeline';
import { ApplicantReasons } from './ApplicantReasons';
import { withSelectedReasons } from './selectedReasons.testFixtures';
import { hasSelectedReason } from './selectedReasonsModel';
import { inferCategories } from './reasonCategories';

vi.mock('../../api/client', () => ({ apiGet: vi.fn() }));
vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
afterEach(() => { cleanup(); });

function demo(): JobCopyRecord {
  const job = jobs.find(item => item.id === 'demo-job-001');
  if (!job) throw new Error('Missing demo job');
  return job;
}
function demoReasons(): ApplicantReasonCollection {
  const reasons = demo().applicantReasons;
  if (!reasons) throw new Error('Missing demo reasons');
  return reasons;
}
const LEGACY = ['oubodouki', 'ouboriyuu_baitaikisai', 'ouboriyuu_hiaringu'];
/** The shape of a stored file written before the categories were read. */
function legacy(): ApplicantReasonCollection {
  const reasons = demoReasons();
  return { ...reasons, selections: null, items: reasons.items.filter(item => LEGACY.includes(item.sourceProperty)).map(item => ({ ...item, applicant: null })),
    sourceCounts: Object.fromEntries(Object.entries(reasons.sourceCounts).filter(([property]) => LEGACY.includes(property))) };
}

describe('legacy sources in the listing detail', () => {
  it('keeps unread selected reasons distinct from zero, on both the tab and timeline', () => {
    const job = { ...demo(), applicantReasons: legacy() };
    const { unmount } = render(<ApplicantReasons job={job} />);
    expect(screen.getByText('選択式の応募理由は未取得です。')).toBeTruthy();
    expect(screen.queryAllByRole('table')).toHaveLength(0);
    unmount();
    render(<JobTimeline job={job} marketMode="demo" />);
    const lane = screen.getByRole('group', { name: '応募理由' });
    expect(lane.textContent).toContain('選択式の応募理由は未取得です。');
    expect(lane.querySelectorAll('button')).toHaveLength(0);
    expect(screen.getByRole('region', { name: '期間ごとの応募理由' }).textContent).not.toContain('推定');
  });
});

describe('the 応募理由 lane when every reason was left out', () => {
  it('says selected records were excluded, rather than claiming there were no records', () => {
    const job = withSelectedReasons(demo());
    const keys = (job.applicationRows ?? []).filter(hasSelectedReason).map(row => row.key);
    if (!job.applicantReasons) throw new Error('Missing reasons');
    render(<JobTimeline job={{ ...job, applicantReasons: { ...job.applicantReasons, multiListingApplicants: keys } }} marketMode="demo" />);
    const lane = screen.getByRole('group', { name: '応募理由' });
    expect(lane.textContent).toContain('この求人だけに関連する選択式の応募理由の記録はありません（ほかの求人にも関連する9件を除く）');
    expect(lane.querySelectorAll('button')).toHaveLength(0);
    const tables = within(screen.getByRole('region', { name: '期間ごとの応募理由の数値' })).getAllByRole('table');
    expect(tables[0]?.closest('section')?.textContent).toContain('応募動機を聞けた応募 0件／この求人の全応募 28件');
  });
});

describe('texts cut by the read limit', () => {
  it('keeps selected counts complete and does not claim unread texts are absent', () => {
    const job = withSelectedReasons(demo());
    if (!job.applicantReasons) throw new Error('Missing reasons');
    const truncated = { ...job, applicantReasons: { ...job.applicantReasons, truncated: true, items: [] } };
    const { unmount } = render(<JobTimeline job={truncated} marketMode="demo" />);
    expect(within(screen.getByRole('group', { name: '応募理由' })).getAllByRole('button').map(chip => chip.textContent)).toEqual(['選択式の記録 3件', '選択式の記録 5件']);
    expect(screen.queryByText(/期間ごとの応募理由の数は実際より少ない/u)).toBeNull();
    unmount();
    render(<ApplicantReasons job={truncated} />);
    expect(screen.getByText('文章は読み込み上限のため一部を表示しています。')).toBeTruthy();
    expect(screen.getByText('読み込み上限により、表示できる応募理由の文章はありません。')).toBeTruthy();
    expect(screen.queryByText('応募理由の文章は記録されていません。')).toBeNull();
    const notes = [...document.querySelectorAll('details > p')].map(node => node.textContent).filter(text => text.includes('＊＊'));
    expect(notes[0]).toContain('それ以外の個人情報が残っていることがあります');
  });
});

describe('the keyword dictionary after normalizing the keywords once', () => {
  it.each([
    ['時給が高い', ['給与']],
    ['家から近い', ['勤務地']],
    ['介護の仕事がしたい', ['職種興味']],
    ['大手で安心', ['会社規模']],
    ['ＳＮＳで見た', []],
  ])('%s', (text, expected) => {
    expect(inferCategories(text)).toEqual(expected);
  });
});
