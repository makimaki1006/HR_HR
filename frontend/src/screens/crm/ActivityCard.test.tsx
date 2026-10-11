// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { WorkspaceActivity } from '../../generated/WorkspaceActivity';
import { ActivityCard } from './ActivityCard';
import { sanitizeHtml, splitQuotedHtml, splitQuotedText } from './activityHtml';

afterEach(cleanup);

const OWNERS: ReadonlyMap<string, string> = new Map([['9001', '架空 花子']]);

function act(over: Partial<WorkspaceActivity>): WorkspaceActivity {
  return {
    id: '1', kind: 'email', timestamp: '2026-09-30T01:00:00Z', title: 'ご挨拶(架空)', body: null, direction: 'EMAIL', status: null,
    duration_ms: null, owner_id: '9001', source: null, via: 'deal', via_id: 'd1', ...over,
  };
}
const RICH = { body_html: null, body_full: null, from_name: null, from_email: null, to: [], cc: [], thread_id: null, attachments_count: null, start_time: null, end_time: null, location: null, recording_url: null };

describe('sanitizeHtml', () => {
  it('script / onerror / javascript: / style を除き、基本の書式とリンクは残す', () => {
    const out = sanitizeHtml('<p>a<script>alert(1)</script><b>太字</b><br><img src="https://example.invalid/a.png" onerror="x()"><img src="http://example.invalid/b.png"><a href="javascript:alert(1)">悪</a><a href="https://example.invalid/">良</a><style>p{color:red}</style><iframe src="https://example.invalid"></iframe><form><input></form></p>');
    expect(out).not.toMatch(/<script|onerror|javascript:|<style|<iframe|<form|<input/i);
    expect(out).toContain('<b>太字</b>');
    expect(out).toContain('<br>');
    expect(out).toContain('href="https://example.invalid/"');
    expect(out).toContain('rel="noopener noreferrer"');
    expect(out).toContain('target="_blank"');
    // https の画像だけ残り、no-referrer と lazy が付く。http の画像は消える
    expect(out).toContain('src="https://example.invalid/a.png"');
    expect(out).toContain('referrerpolicy="no-referrer"');
    expect(out).toContain('loading="lazy"');
    expect(out).not.toContain('b.png');
  });
});

describe('引用の折りたたみ', () => {
  it('blockquote 以降を引用に分ける', () => {
    const { main, quoted } = splitQuotedHtml('<p>本文です</p><blockquote>前のメール</blockquote>');
    expect(main).toContain('本文です');
    expect(main).not.toContain('前のメール');
    expect(quoted).toContain('前のメール');
  });
  it('Original Message の見出し以降を引用に分ける (平文)', () => {
    const r = splitQuotedText('了解しました\n\n-----Original Message-----\nFrom: x\n前の内容');
    expect(r.main).toBe('了解しました');
    expect(r.quoted).toContain('前の内容');
  });
  it('> で始まる行を引用にする。全部が引用なら分けない', () => {
    expect(splitQuotedText('返信です\n> 前の文').quoted).toBe('> 前の文');
    expect(splitQuotedText('> 前の文').quoted).toBe('');
  });
});

describe('ActivityCard', () => {
  it('メールは件名・送信/受信・差出人 → 宛先を見出しに出し、全文表示で本文を、引用は別の開閉で出す', () => {
    const a = act({
      direction: 'INCOMING_EMAIL',
      rich: { ...RICH, body_html: '<p>本文の一行目</p><p>署名 架空</p><blockquote>引用された古い文</blockquote>', from_name: '山田 太郎', from_email: 'taro@example.invalid', to: ['me@example.invalid'], cc: ['cc@example.invalid'] },
    });
    render(<ul><ActivityCard a={a} ownerNames={OWNERS} /></ul>);
    expect(screen.getByTestId('activity-title').textContent).toBe('ご挨拶(架空)');
    expect(screen.getByTestId('activity-direction').textContent).toBe('受信');
    expect(screen.getByTestId('activity-addr').textContent).toContain('山田 太郎 <taro@example.invalid> → me@example.invalid');
    expect(screen.getByTestId('activity-addr').textContent).toContain('CC: cc@example.invalid');
    expect(screen.queryByTestId('activity-body')).toBeNull();
    expect(screen.getByTestId('activity-preview').textContent).toContain('本文の一行目');
    fireEvent.click(screen.getByRole('button', { name: '全文を表示' }));
    expect(screen.getByTestId('activity-body').textContent).toContain('署名 架空');
    expect(screen.queryByTestId('activity-quoted')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: '以前のやりとりを表示' }));
    expect(screen.getByTestId('activity-quoted').textContent).toContain('引用された古い文');
  });

  it('送信メールは「送信」と表示する', () => {
    render(<ul><ActivityCard a={act({ rich: { ...RICH, body_html: '<p>x</p>', to: ['a@example.invalid'] } })} ownerNames={OWNERS} /></ul>);
    expect(screen.getByTestId('activity-direction').textContent).toBe('送信');
  });

  it('HTML が無い本文は改行を保って表示する', () => {
    const a = act({ kind: 'note', title: null, direction: null, body: '一行目', rich: { ...RICH, body_full: '一行目\n二行目\n三行目\n四行目\n五行目' } });
    render(<ul><ActivityCard a={a} ownerNames={OWNERS} /></ul>);
    fireEvent.click(screen.getByRole('button', { name: '全文を表示' }));
    expect(screen.getByTestId('activity-body').textContent).toBe('一行目\n二行目\n三行目\n四行目\n五行目');
    expect(screen.getByTestId('activity-body').className).toContain('wd-card-text');
  });

  it('短いメモには「全文を表示」を出さない', () => {
    render(<ul><ActivityCard a={act({ kind: 'note', title: null, direction: null, body: '短いメモ' })} ownerNames={OWNERS} /></ul>);
    expect(screen.queryByRole('button', { name: '全文を表示' })).toBeNull();
    expect(screen.getByTestId('activity-preview').textContent).toBe('短いメモ');
  });

  it('悪意のある HTML は描かれない', () => {
    const a = act({ rich: { ...RICH, body_html: '<p>安全</p><img src="https://example.invalid/x.png" onerror="window.__pwned=1"><script>window.__pwned=1</script>' } });
    render(<ul><ActivityCard a={a} ownerNames={OWNERS} /></ul>);
    fireEvent.click(screen.getByRole('button', { name: '全文を表示' }));
    const body = screen.getByTestId('activity-body');
    expect(body.querySelector('script')).toBeNull();
    expect(body.innerHTML).not.toContain('onerror');
    expect((window as unknown as { __pwned?: number }).__pwned).toBeUndefined();
  });

  it('ミーティングは開始〜終了と場所、通話は録音リンクを出す', () => {
    const m = act({ kind: 'meeting', direction: null, rich: { ...RICH, start_time: '2026-10-01T05:00:00Z', end_time: '2026-10-01T05:30:00Z', location: '会議室(架空)' } });
    render(<ul><ActivityCard a={m} ownerNames={OWNERS} /></ul>);
    expect(document.body.textContent).toContain('2026/10/01 14:00 〜 14:30');
    expect(document.body.textContent).toContain('場所: 会議室(架空)');
    cleanup();
    const c = act({ kind: 'call', direction: 'OUTBOUND', rich: { ...RICH, recording_url: 'https://example.invalid/rec.mp3' } });
    render(<ul><ActivityCard a={c} ownerNames={OWNERS} /></ul>);
    const link = screen.getByRole('link', { name: '録音を聞く' });
    expect(link.getAttribute('rel')).toBe('noopener noreferrer');
  });
});
