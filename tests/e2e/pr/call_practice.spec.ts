import { expect, test, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { login } from './helpers/login';

/**
 * 架電の練習 (React 版 /app/call-practice) の PR 用 E2E。
 *
 * - 画面は ?mode=fixture (お客さん役もサーバも呼ばず、決まった返事で動く)。実際の LLM は呼ばない。
 * - 本物のマイクは使えないので、ブラウザの SpeechRecognition を偽物に差し替え、認識結果を差し込む。
 *   画面の音声入力の部品 (話している途中の文字・確定・自動で送る) は本物のまま動く。
 * - サーバ側の認可は、パスワードログインのまま API を叩いて 403 を確かめる (管理者だけ・Google ログイン必須)。
 * - 画面の写真は環境変数 CP_SHOT_DIR があるときだけ撮る。
 */

async function installFakeMic(page: Page): Promise<void> {
  await page.addInitScript(() => {
    interface W extends Window {
      SpeechRecognition?: unknown;
      __rec?: { onresult: ((e: unknown) => void) | null; onend: (() => void) | null };
      __emit?: (items: { t: string; final: boolean }[]) => void;
    }
    const w = window as W;
    w.SpeechRecognition = class {
      lang = '';
      continuous = false;
      interimResults = false;
      onresult: ((e: unknown) => void) | null = null;
      onerror: ((e: { error: string }) => void) | null = null;
      onend: (() => void) | null = null;
      constructor() {
        w.__rec = this;
      }
      start(): void {}
      stop(): void {
        this.onend?.();
      }
      abort(): void {
        this.onend?.();
      }
    };
    w.__emit = (items) => {
      const results = items.map((i) => Object.assign([{ transcript: i.t }], { isFinal: i.final }));
      w.__rec?.onresult?.({ resultIndex: 0, results });
    };
  });
}

async function emit(page: Page, items: { t: string; final: boolean }[]): Promise<void> {
  await page.evaluate((x) => {
    (window as unknown as { __emit: (i: typeof x) => void }).__emit(x);
  }, items);
}

async function shot(page: Page, name: string): Promise<void> {
  const dir = process.env.CP_SHOT_DIR;
  if (dir === undefined || dir === '') return;
  fs.mkdirSync(dir, { recursive: true });
  await page.screenshot({ path: path.join(dir, name), fullPage: true });
}

test.describe('架電の練習 /app/call-practice', () => {
  test('パスワードログインでは API が使えない (管理者・Google ログインだけ)', async ({ page }) => {
    await login(page);
    const res = await page.request.get('/api/call-practice/config', { headers: { Accept: 'application/json' } });
    expect(res.status()).toBe(403);
    expect(((await res.json()) as { error_kind: string }).error_kind).toBe('google_login_required');
  });

  test('声で聞くと途中の文字が出て、話し終わりに自動で送られ、記録して終えると結果が出る', async ({ page }) => {
    await installFakeMic(page);
    await login(page);
    await page.goto('/app/call-practice?mode=fixture');
    await expect(page.getByRole('heading', { name: '架電の練習' })).toBeVisible();
    await expect(page.getByTestId('practice-intro')).toContainText('アポの日時が決まった直後から始まります。');
    await page.getByRole('button', { name: '練習を始める' }).click();
    await expect(page.getByTestId('turn-1')).toContainText('はい、ではその日でお願いします');
    await expect(page.getByTestId('mic-state')).toHaveText('聞き取っています');

    await emit(page, [{ t: '直近3か月以内で', final: false }]);
    await expect(page.getByTestId('interim')).toHaveText('聞き取り中: 直近3か月以内で');
    const q1 = '直近3か月以内で、採用が必要な職種はありますか？';
    await emit(page, [{ t: q1, final: true }]);
    await expect(page.getByTestId('turn-2')).toContainText(q1);
    await expect(page.getByTestId('turn-3')).toContainText('ドライバーを2名、3か月以内に採りたいですね');
    await expect(page.getByTestId('interim')).toHaveText('');

    const q2 = '今の採用で困っていることと、どのくらい早く解決したいかを教えていただけますか？';
    await emit(page, [{ t: q2, final: true }]);
    await expect(page.getByTestId('turn-5')).toContainText('前任が辞めてしまって、早く埋めたいんです');
    await shot(page, 'call-practice-practice.png');

    // 記録欄
    await page.locator('#rec-hiring_3m').fill('ドライバー 2名');
    await page.locator('#rec-urgency').selectOption('not_so');
    await page.getByLabel('聞けていない').nth(4).check(); // 検討
    await page.getByLabel('熱い', { exact: true }).check();
    await page.getByTestId('finish').click();
    await expect(page.getByTestId('blank-confirm')).toContainText('空欄は「聞けていない」として採点します');
    await page.getByRole('button', { name: 'このまま進む' }).click();

    const result = page.getByTestId('practice-result');
    await expect(result).toBeVisible();
    await expect(page.getByTestId('result-item-hiring')).toContainText('取れた');
    await expect(page.getByTestId('result-item-hiring')).toContainText('3か月以内の職種と人数: ドライバー 2名');
    await expect(page.getByTestId('result-item-issue')).toContainText('記録が違う');
    await expect(page.getByTestId('result-item-cost')).toContainText('聞けていないと記録');
    await expect(page.getByTestId('question-count')).toContainText('あなたの質問は 2 問でした。');
    await expect(page.getByTestId('not-heard-left')).toContainText('「聞けていない」が残った項目: 1 件(③ 検討と予算)');
    await expect(page.getByTestId('heat-summary')).toContainText('あなたの見立て「熱い」 / お客さんの型の熱さ「熱い」');
    await expect(page.getByTestId('heat-marks')).toHaveText('隠れた答えにあった熱い印は 5 個、そのうち聞き出せたのは 2 個でした。');
    await expect(page.getByTestId('persona-label')).toHaveText('熱い型');
    await expect(page.getByTestId('commentary')).toContainText('2 番「' + q1 + '」');
    await shot(page, 'call-practice-result.png');
  });

  test('音声認識が使えないブラウザでは文字入力に切り替える旨が出て、文字で送れる', async ({ page }) => {
    await page.addInitScript(() => {
      Object.defineProperty(window, 'SpeechRecognition', { value: undefined, configurable: true });
      Object.defineProperty(window, 'webkitSpeechRecognition', { value: undefined, configurable: true });
    });
    await login(page);
    await page.goto('/app/call-practice?mode=fixture');
    await page.getByRole('button', { name: '練習を始める' }).click();
    await expect(page.getByTestId('speech-notice')).toHaveText('このブラウザでは音声入力が使えません。文字で入力してください。');
    await expect(page.getByTestId('mic-toggle')).toHaveCount(0);
    await page.getByTestId('draft').fill('手で打った質問です');
    await page.getByRole('button', { name: '送る' }).click();
    await expect(page.getByTestId('turn-2')).toContainText('手で打った質問です');
    await expect(page.getByTestId('turn-3')).toContainText('ドライバーを2名、3か月以内に採りたいですね');
  });
});
