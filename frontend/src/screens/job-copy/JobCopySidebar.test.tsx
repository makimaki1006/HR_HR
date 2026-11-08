// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { JobCopySidebar, useJobCopySidebar } from './JobCopySidebar';
afterEach(cleanup);
it('左の帯は「☰ 求人」の 1 つのボタンにし、求人名は補足に残す', () => {
  function Sidebar() { return <JobCopySidebar {...useJobCopySidebar()} title="合成の配送求人"><p>合成一覧</p></JobCopySidebar>; }
  render(<Sidebar />);
  const button = screen.getByRole('button', { name: '求人一覧を開く' });
  expect(button.textContent).toBe('☰求人');
  expect(button.getAttribute('title')).toBe('合成の配送求人');
});
