// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { JobCopySidebar, useJobCopySidebar } from './JobCopySidebar';
afterEach(cleanup);
it('左の帯は「求人を探す」の 1 つのボタンにし、求人名は補足に残す。マウスが通るだけでは開かず、押したときだけ開く', () => {
  function Sidebar() { return <JobCopySidebar {...useJobCopySidebar()} title="合成の配送求人"><p>合成一覧</p></JobCopySidebar>; }
  render(<Sidebar />);
  const button = screen.getByRole('button', { name: '左の一覧で求人を探す' });
  expect(button.textContent).toBe('求人を探す');
  expect(button.getAttribute('title')).toBe('合成の配送求人');
  fireEvent.mouseEnter(button.closest('.jc-sidebar') ?? button);
  fireEvent.mouseOver(button);
  expect(button.getAttribute('aria-expanded')).toBe(window.matchMedia('(min-width: 801px)').matches ? 'false' : 'true');
  fireEvent.click(button);
  expect(button.getAttribute('aria-expanded')).toBe('true');
  expect(screen.getByRole('button', { name: '開いたままにする' }).getAttribute('aria-pressed')).toBe('false');
});
