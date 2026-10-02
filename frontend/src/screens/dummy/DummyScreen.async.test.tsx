// @vitest-environment happy-dom
// Async behaviour of DummyScreen: 401 goes to the login page, and a response that arrives after
// the request was aborted (StrictMode runs the effect twice) must not overwrite the newer one.
import { StrictMode } from 'react';
import { act, cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../../shell/navigation', () => ({ redirectToLogin: vi.fn() }));

import { redirectToLogin } from '../../shell/navigation';
import { DummyScreen } from './DummyScreen';

const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

beforeEach(() => {
  vi.mocked(redirectToLogin).mockClear();
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe('DummyScreen async', () => {
  it('HTTP 401 calls redirectToLogin once', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn<typeof fetch>(() => Promise.resolve(json({ error: 'auth_required' }, 401))),
    );
    render(<DummyScreen />);
    await waitFor(() => {
      expect(redirectToLogin).toHaveBeenCalledTimes(1);
    });
  });

  it('a late response of an aborted request does not overwrite the newer one', async () => {
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
    render(
      <StrictMode>
        <DummyScreen />
      </StrictMode>,
    );
    expect(resolvers).toHaveLength(2);
    await act(async () => {
      resolvers[1]?.(json({ message: 'NEW', server_time: 't2' }));
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(screen.getByTestId('ping-message').textContent).toBe('NEW');
    await act(async () => {
      resolvers[0]?.(json({ message: 'OLD', server_time: 't1' }));
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(screen.getByTestId('ping-message').textContent).toBe('NEW');
  });
});
