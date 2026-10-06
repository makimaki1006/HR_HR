// @vitest-environment happy-dom
import { createElement } from 'react';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { ImageGallery } from './ImageGallery';

afterEach(cleanup);

describe('image request lifecycle', () => {
  it('keeps loading distinct from failure and zero, then retries the same evidence URL', () => {
    const url = '/api/job-copy/snapshot-image?listing_id=30&version=0&slot=1&image_hash=' + 'a'.repeat(64);
    render(createElement(ImageGallery, { title: '合成画像', images: [{ id: 'synthetic', url, caption: '合成写真', contentHash: 'a'.repeat(64) }] }));
    const region = screen.getByRole('region', { name: '合成画像' });
    const open = within(region).getByRole<HTMLButtonElement>('button', { name: '画像1を拡大: 合成写真' });
    expect(within(region).getByRole('status').textContent).toContain('画像を取得中');
    expect(open.disabled).toBe(true);
    expect(within(region).queryByRole('alert')).toBeNull();
    const initial = within(region).getByRole<HTMLImageElement>('img');
    fireEvent.error(initial);
    expect(within(region).queryByRole('status')).toBeNull();
    expect(within(region).getByRole('alert').textContent).toContain('画像なし・削除とは判定していません');
    expect(region.textContent).not.toContain('画像は0点');
    fireEvent.click(within(region).getByRole('button', { name: '画像を再読み込み: 合成写真' }));
    const retried = within(region).getByRole<HTMLImageElement>('img');
    expect(retried).not.toBe(initial);
    expect(retried.getAttribute('src')).toBe(url);
    expect(within(region).getByRole('status').textContent).toContain('画像を取得中');
    expect(open.disabled).toBe(true);
    fireEvent.load(retried);
    expect(within(region).queryByRole('status')).toBeNull();
    expect(within(region).queryByRole('alert')).toBeNull();
    expect(open.disabled).toBe(false);
    expect(retried.getAttribute('src')).toBe(url);
  });
});
