// 応募者ジャーニー診断へ求人原文を引き継ぐ (旧 static/jobgen.html の saveSourceHandoff)。
// 別タブ (同一オリジン) で開くため localStorage を使う。渡すのは正規化済みの本文と
// 職種名候補だけで、元ファイルそのものは持ち出さない。
export const HANDOFF_KEY = 'hrhr-jobgen-source-handoff';
/** 保存失敗でこの画面を壊さないための上限。 */
export const HANDOFF_MAX_CHARS = 400000;

export function saveSourceHandoff(sourceText: string, titleHint: string, now: Date): void {
  try {
    if (typeof localStorage === 'undefined') return;
    if (!sourceText || sourceText.length > HANDOFF_MAX_CHARS) {
      localStorage.removeItem(HANDOFF_KEY);
      return;
    }
    localStorage.setItem(
      HANDOFF_KEY,
      JSON.stringify({ text: sourceText, title: titleHint, saved_at: now.toISOString() }),
    );
  } catch {
    // 容量超過・プライベートモード等。引き継ぎは補助機能なので、この画面の動作は止めない。
  }
}
