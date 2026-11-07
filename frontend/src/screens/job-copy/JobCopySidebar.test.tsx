// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { JobCopySidebar, useJobCopySidebar } from './JobCopySidebar';
afterEach(cleanup);
it('短い求人ラベルと開くボタンを表示し、求人名は補足に残す', () => {
  function Sidebar() { return <JobCopySidebar {...useJobCopySidebar()} title="合成の配送求人"><p>合成一覧</p></JobCopySidebar>; }
  render(<Sidebar />);
  expect(document.querySelector('.jc-sidebar-current')?.textContent).toBe('求人');
  expect(document.querySelector('.jc-sidebar-current')?.getAttribute('title')).toBe('合成の配送求人');
  expect(screen.getByRole('button', { name: '求人一覧を開く' }).textContent).toBe('☰開く');
});
