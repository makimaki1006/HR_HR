// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { TalkScreen, parseTalkRoute, talkHref } from './TalkScreen';

afterEach(() => {
  cleanup();
  window.history.replaceState(null, '', '/');
});

describe('/app/talk', () => {
  it('view なしのクエリを読み、view を付けない URL を作る', () => {
    const r = parseTalkRoute('?from=2026-10-19&to=2026-10-20&team=A&owner=7&call=9');
    expect(r).toEqual({ view: 'talk', from: '2026-10-19', to: '2026-10-20', team: 'A', owner: '7', call: '9' });
    window.history.replaceState(null, '', '/app/talk');
    expect(talkHref(r)).toBe('?from=2026-10-19&to=2026-10-20&team=A&owner=7&call=9');
  });

  it('架空サンプルで TalkPanel が出て、管理者用のタブは出ない', async () => {
    window.history.replaceState(null, '', '/app/talk?mode=fixture&from=2026-10-19&to=2026-10-19');
    render(<TalkScreen />);
    expect(await screen.findByTestId('talk-table')).toBeTruthy();
    expect(screen.getByText('トークの振り返り', { selector: 'h1' })).toBeTruthy();
    expect(screen.queryByText('利用状況')).toBeNull();
    expect(screen.queryByText('ユーザー一覧')).toBeNull();
    expect(screen.queryByText('HubSpot')).toBeNull();
  });
});
