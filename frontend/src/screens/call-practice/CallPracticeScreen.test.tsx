// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PracticeTurnResponse } from '../../generated/PracticeTurnResponse';
import type { PracticeApi } from './api';
import { PracticeError } from './api';
import { CallPracticeScreen } from './CallPracticeScreen';
import { createFixtureApi, FIXTURE_OPENING, FIXTURE_REPLIES } from './fixtureApi';
import { DENIED_MESSAGE, UNSUPPORTED_MESSAGE } from './useSpeechInput';

interface Item {
  t: string;
  final: boolean;
}

/** 本物のマイクの代わり。認識結果を差し込める */
class FakeRecognition {
  static last: FakeRecognition | null = null;
  lang = '';
  continuous = false;
  interimResults = false;
  onresult: ((e: unknown) => void) | null = null;
  onerror: ((e: { error: string }) => void) | null = null;
  onend: (() => void) | null = null;
  start(): void {
    FakeRecognition.last = this;
  }
  stop(): void {
    this.onend?.();
  }
  abort(): void {
    this.onend?.();
  }
  emit(items: Item[], resultIndex = 0): void {
    const results = items.map((i) => Object.assign([{ transcript: i.t }], { isFinal: i.final }));
    this.onresult?.({ resultIndex, results });
  }
}

function installSpeech(): void {
  (window as unknown as { SpeechRecognition: typeof FakeRecognition }).SpeechRecognition = FakeRecognition;
}

beforeEach(() => {
  FakeRecognition.last = null;
  window.history.replaceState(null, '', '/app/call-practice?mode=fixture');
});

afterEach(() => {
  cleanup();
  delete (window as unknown as { SpeechRecognition?: unknown }).SpeechRecognition;
  window.history.replaceState(null, '', '/');
  vi.restoreAllMocks();
});

async function startPractice(): Promise<void> {
  fireEvent.click(await screen.findByText('練習を始める'));
  await screen.findByTestId('practice-room');
}

describe('/app/call-practice', () => {
  it('使えないときはやさしい日本語の理由を出す', async () => {
    const api: PracticeApi = { ...createFixtureApi(), config: () => Promise.resolve({ enabled: false, unavailable_reason: 'いまはお客さん役を呼び出せません。管理者に連絡してください。', max_turns: 30, max_minutes: 15 }) };
    render(<CallPracticeScreen api={api} />);
    expect((await screen.findByTestId('practice-unavailable')).textContent).toBe('いまはお客さん役を呼び出せません。管理者に連絡してください。');
  });

  it('話した文字を途中から出し、確定したら自動で送り、返事を流して出す', async () => {
    installSpeech();
    render(<CallPracticeScreen api={createFixtureApi(1)} />);
    await startPractice();
    expect(screen.getByTestId('turn-1').textContent).toContain(FIXTURE_OPENING);
    const rec = FakeRecognition.last;
    expect(rec).not.toBeNull();
    expect(rec?.lang).toBe('ja-JP');
    expect(rec?.interimResults && rec.continuous).toBe(true);
    const say = '直近3か月以内で、採用が必要な職種はありますか？';
    act(() => {
      rec?.emit([{ t: '直近3か月以内で', final: false }]);
    });
    expect(screen.getByTestId('interim').textContent).toBe('聞き取り中: 直近3か月以内で');
    act(() => {
      rec?.emit([{ t: say, final: true }]);
    });
    await waitFor(() => {
      expect(screen.getByTestId('turn-2').textContent).toContain(say);
    }, { timeout: 4000 });
    await waitFor(() => {
      expect(screen.getByTestId('turn-3').textContent).toContain(FIXTURE_REPLIES[0]);
    });
    expect(screen.getByTestId('interim').textContent).toBe('');
  });

  it('無音が続いたら確定前の文字でも送る', async () => {
    installSpeech();
    render(<CallPracticeScreen api={createFixtureApi(1)} />);
    await startPractice();
    act(() => {
      FakeRecognition.last?.emit([{ t: '月額定額なら検討できそうですか', final: false }]);
    });
    await waitFor(() => {
      expect(screen.getByTestId('turn-2').textContent).toContain('月額定額なら検討できそうですか');
    }, { timeout: 5000 });
  });

  it('お客さん役が返事をしている間は、聞き取った文字を送らない', async () => {
    installSpeech();
    let release: ((r: PracticeTurnResponse) => void) | null = null;
    const calls: string[] = [];
    const base = createFixtureApi(1);
    const api: PracticeApi = {
      ...base,
      turn: (_id, req) => {
        calls.push(req.text);
        return new Promise<PracticeTurnResponse>((resolve) => {
          release = resolve;
        });
      },
    };
    render(<CallPracticeScreen api={api} />);
    await startPractice();
    act(() => {
      FakeRecognition.last?.emit([{ t: 'ひとつめの質問です', final: true }]);
    });
    await waitFor(() => {
      expect(calls).toEqual(['ひとつめの質問です']);
    }, { timeout: 4000 });
    // 返事の途中で差し込んだ認識結果は捨てられる
    act(() => {
      FakeRecognition.last?.emit([{ t: '返事の途中で話した言葉', final: true }]);
    });
    await new Promise((r) => setTimeout(r, 1200));
    expect(calls).toEqual(['ひとつめの質問です']);
    act(() => {
      release?.({
        rep_turn: { n: 2, speaker: 'rep', text: 'ひとつめの質問です' },
        customer_turn: { n: 3, speaker: 'customer', text: 'はい' },
        ended: false,
      });
    });
    await waitFor(() => {
      expect(screen.getByTestId('turn-3').textContent).toContain('はい');
    });
    expect(screen.queryByText('返事の途中で話した言葉')).toBeNull();
    expect(calls).toHaveLength(1);
  });

  it('音声認識が使えないブラウザでは文字入力に切り替える旨を出し、文字で送れる', async () => {
    render(<CallPracticeScreen api={createFixtureApi(1)} />);
    await startPractice();
    expect(screen.getByTestId('speech-notice').textContent).toBe(UNSUPPORTED_MESSAGE);
    expect(screen.queryByTestId('mic-toggle')).toBeNull();
    fireEvent.change(screen.getByTestId('draft'), { target: { value: '手で打った質問です' } });
    fireEvent.click(screen.getByText('送る'));
    await waitFor(() => {
      expect(screen.getByTestId('turn-2').textContent).toContain('手で打った質問です');
    });
  });

  it('マイクを許可されなかったときは文字入力に切り替える旨を出す', async () => {
    installSpeech();
    render(<CallPracticeScreen api={createFixtureApi(1)} />);
    await startPractice();
    act(() => {
      FakeRecognition.last?.onerror?.({ error: 'not-allowed' });
    });
    expect(screen.getByTestId('speech-notice').textContent).toBe(DENIED_MESSAGE);
    expect(screen.getByTestId('draft')).toBeTruthy();
  });

  it('返事が取れなかったら発言を入力欄に残してやさしい文言を出す', async () => {
    const api: PracticeApi = {
      ...createFixtureApi(1),
      turn: () => Promise.reject(new PracticeError('llm_busy', 'お客さん役が返事できませんでした。もう一度送ってください。')),
    };
    render(<CallPracticeScreen api={api} />);
    await startPractice();
    fireEvent.change(screen.getByTestId('draft'), { target: { value: 'こんにちは' } });
    fireEvent.click(screen.getByText('送る'));
    expect((await screen.findByTestId('practice-error')).textContent).toBe('お客さん役が返事できませんでした。もう一度送ってください。');
    expect(screen.getByTestId<HTMLInputElement>('draft').value).toBe('こんにちは');
  });

  it('記録して終えると、区分・記録・本当の答えを具体値で並べて型を明かす', async () => {
    render(<CallPracticeScreen api={createFixtureApi(1)} />);
    await startPractice();
    for (const text of ['職種はありますか', '困っていることはありますか']) {
      fireEvent.change(screen.getByTestId('draft'), { target: { value: text } });
      fireEvent.click(screen.getByText('送る'));
      await waitFor(() => {
        expect(screen.getByTestId<HTMLInputElement>('draft').disabled).toBe(false);
      });
    }
    // 見立てが無いと終えられない
    fireEvent.click(screen.getByTestId('finish'));
    expect(screen.getByTestId('practice-error').textContent).toBe('見立てを選んでから終えてください。');
    fireEvent.click(screen.getByLabelText('熱い'));
    fireEvent.click(screen.getByTestId('finish'));
    expect(screen.getByTestId('blank-confirm').textContent).toContain('空欄は「聞けていない」として採点します');
    fireEvent.click(screen.getByText('このまま進む'));
    await screen.findByTestId('practice-result');
    const hiring = within(screen.getByTestId('result-item-hiring'));
    expect(hiring.getByText('取れた')).toBeTruthy();
    expect(hiring.getAllByText('3か月以内の職種と人数: ドライバー 2名').length).toBe(2);
    expect(within(screen.getByTestId('result-item-cost')).getByText('聞けていないと記録')).toBeTruthy();
    expect(screen.getByTestId('question-count').textContent).toContain('あなたの質問は 2 問でした。');
    expect(screen.getByTestId('not-heard-left').textContent).toContain('1 件(③ 検討と予算)');
    expect(screen.getByTestId('persona-label').textContent).toBe('熱い型');
    expect(screen.getByTestId('heat-marks').textContent).toBe('隠れた答えにあった熱い印は 5 個、そのうち聞き出せたのは 2 個でした。');
    expect(screen.getByText('同じ型でもう一度')).toBeTruthy();
  });
});
