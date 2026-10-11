// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { textHtml } from './emailApi';

describe('textHtml links', () => {
  it('links both protocols after escaping and keeps line breaks and query parameters', () => {
    expect(textHtml('資料 https://example.invalid/docs?a=1&b=2\nhttp://example.invalid/next')).toBe('<p>資料 <a href="https://example.invalid/docs?a=1&amp;b=2">https://example.invalid/docs?a=1&amp;b=2</a><br><a href="http://example.invalid/next">http://example.invalid/next</a></p>');
  });
  it.each(['。', '、', '）', ')', '」', '』', '】', '.', ',', '!?'])('keeps trailing %s outside the link', punctuation => {
    expect(textHtml(`https://example.invalid/docs${punctuation}`)).toBe(`<p><a href="https://example.invalid/docs">https://example.invalid/docs</a>${punctuation}</p>`);
  });
  it('keeps surrounding Japanese parentheses and the following sentence as text', () => {
    expect(textHtml('資料（https://example.invalid/docs）。ご確認ください。')).toBe('<p>資料（<a href="https://example.invalid/docs">https://example.invalid/docs</a>）。ご確認ください。</p>');
  });
  it('does not cut the semicolon off an escaped ampersand', () => {
    expect(textHtml('https://example.invalid/?a=1&。')).toBe('<p><a href="https://example.invalid/?a=1&amp;">https://example.invalid/?a=1&amp;</a>。</p>');
  });
  it('cannot turn input quotes, tags or a javascript URL into active HTML', () => {
    const html = textHtml('https://example.invalid/docs" onclick="alert(1) <script>x</script> javascript:alert(2)');
    const doc = new DOMParser().parseFromString(html, 'text/html');
    const links = doc.querySelectorAll('a');
    expect(links).toHaveLength(1);
    expect(links[0]?.getAttribute('href')).toBe('https://example.invalid/docs');
    expect(links[0]?.getAttributeNames()).toEqual(['href']);
    expect(doc.querySelector('script')).toBeNull();
    expect(doc.body.textContent).toBe('https://example.invalid/docs" onclick="alert(1) <script>x</script> javascript:alert(2)');
  });
  it('escapes literal entity-looking input once without creating attributes', () => {
    const html = textHtml('https://example.invalid/?a=1&b=&quot;onclick=bad');
    const doc = new DOMParser().parseFromString(html, 'text/html');
    expect(doc.querySelector('a')?.getAttribute('href')).toBe('https://example.invalid/?a=1&b=&quot;onclick=bad');
    expect(doc.querySelector('a')?.getAttributeNames()).toEqual(['href']);
  });
});
