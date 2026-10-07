import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ImageGallery } from './ImageGallery';

describe('image gallery evidence states', () => {
  it('distinguishes unacquired images from a known zero-image observation', () => {
    const missing = renderToStaticMarkup(createElement(ImageGallery, { title: '取得状態', images: undefined }));
    const empty = renderToStaticMarkup(createElement(ImageGallery, { title: '取得状態', images: [] }));
    expect(missing).toContain('画像は未取得です。画像なし・削除とは判定していません。');
    expect(missing).not.toContain('画像は0点');
    expect(empty).toContain('この版の画像は0点です。');
    expect(empty).not.toContain('<img');
  });
  it('preserves original image slot, caption and an authorized lazy proxy reference', () => {
    const url = '/api/job-copy/image?company_id=10&listing_id=30&manifest_id=synthetic_Manifest-123&slot=2';
    const markup = renderToStaticMarkup(createElement(ImageGallery, { title: '掲載画像', images: [{ id: 'synthetic', caption: '架空の掲載写真', sourceSlot: 2, url }] }));
    expect(markup).toContain('aria-label="画像2を拡大: 架空の掲載写真"');
    expect(markup).toContain('alt="架空の掲載写真"');
    expect(markup).toContain('loading="lazy"');
    expect(markup).toContain('company_id=10&amp;listing_id=30&amp;manifest_id=synthetic_Manifest-123&amp;slot=2');
    expect(markup).not.toContain('画像は未取得');
  });
});
