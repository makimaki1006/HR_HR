// @vitest-environment happy-dom
// Async behaviour of GuideScreen: 401 goes to the login page; a late response of an aborted
// request must not overwrite the newer state (StrictMode runs the effect twice).
import { StrictMode } from 'react';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../../shell/navigation', () => ({ redirectToLogin: vi.fn() }));

import { redirectToLogin } from '../../shell/navigation';
import { GuideScreen } from './GuideScreen';

const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

beforeEach(() => {
  vi.mocked(redirectToLogin).mockClear();
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('GuideScreen async', () => {
  it('HTTP 401 calls redirectToLogin once', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn<typeof fetch>(() => Promise.resolve(json({ error: 'auth_required' }, 401))),
    );
    render(<GuideScreen />);
    await waitFor(() => {
      expect(redirectToLogin).toHaveBeenCalledTimes(1);
    });
  });

  it('a late failure of an aborted request does not overwrite the newer state', async () => {
    const resolvers: ((r: Response) => void)[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn<typeof fetch>(
        () =>
          new Promise<Response>((r) => {
            resolvers.push(r);
          }),
      ),
    );
    const { container } = render(
      <StrictMode>
        <GuideScreen />
      </StrictMode>,
    );
    expect(resolvers).toHaveLength(2);
    // The newer request is still loading; the aborted first one fails late with HTTP 500.
    await act(async () => {
      resolvers[0]?.(json({ x: 1 }, 500));
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(screen.queryByRole('alert')).toBeNull();
    expect(container.textContent).toContain('読み込み中');
  });
});
