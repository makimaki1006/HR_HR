// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { RetryApplicants } from './RetryApplicants';
import { RETRY_APPLICANTS_EVENT } from './listingApplicationDates';

afterEach(() => { cleanup(); });

it('「もう一度読み込む」で、その求人の応募を読み直す合図を出す', () => {
  const heard = vi.fn();
  const listen = (event: Event) => { heard((event as CustomEvent<string>).detail); };
  window.addEventListener(RETRY_APPLICANTS_EVENT, listen);
  render(<RetryApplicants id="555" />);
  fireEvent.click(screen.getByRole('button', { name: 'もう一度読み込む' }));
  window.removeEventListener(RETRY_APPLICANTS_EVENT, listen);
  expect(heard).toHaveBeenCalledWith('555');
});

it('求人の番号が無いときはボタンを出さない', () => {
  render(<RetryApplicants id={undefined} />);
  expect(screen.queryByRole('button')).toBeNull();
});
