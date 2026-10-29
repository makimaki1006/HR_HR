// 採点と振り返り (設計書 §3.5)。上から順に出す。
import type { PracticeResult } from '../../generated/PracticeResult';
import { HEAT_LABEL } from './record';

const STATUS_CLASS: Record<string, string> = {
  got: 'cp-status-good',
  wrong_record: 'cp-status-bad',
  not_heard: 'cp-status-mid',
  empty: 'cp-status-mid',
  recorded_without_answer: 'cp-status-bad',
};

export function ResultView({
  result,
  onAgainSame,
  onAgainOther,
}: {
  result: PracticeResult;
  onAgainSame: () => void;
  onAgainOther: () => void;
}) {
  const turnText = (n: number): string => result.transcript.find((t) => t.n === n)?.text ?? '';
  const heat = result.heat;
  return (
    <div className="cp-result" data-testid="practice-result">
      <h2 className="w8-h2">結果</h2>

      <section className="w8-card cp-section">
        <h3 className="w8-section-title">4 項目の取れ方</h3>
        <div className="w8-table-wrap">
          <table className="cp-table" data-testid="result-items">
            <thead>
              <tr>
                <th>項目</th>
                <th>区分</th>
                <th>あなたの記録</th>
                <th>お客さんの本当の答え</th>
                <th>聞いた発言</th>
              </tr>
            </thead>
            <tbody>
              {result.items.map((i) => (
                <tr key={i.item} data-testid={`result-item-${i.item}`}>
                  <td>{i.label}</td>
                  <td className={STATUS_CLASS[i.status] ?? ''}>{i.status_label}</td>
                  <td>{i.your_record}</td>
                  <td>{i.truth}</td>
                  <td>{i.turns.length === 0 ? 'なし' : i.turns.map((n) => `${String(n)} 番`).join('、')}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p data-testid="not-heard-left">
          「聞けていない」が残った項目: {result.not_heard_left} 件
          {result.not_heard_items.length > 0 ? `(${result.not_heard_items.join('、')})` : ''}
        </p>
        {result.asked_price && <p className="cp-warn">金額を直接聞いた発言がありました。月額定額を先に伝えてから、検討できそうかを聞きます。</p>}
      </section>

      <section className="w8-card cp-section">
        <h3 className="w8-section-title">嫌がられた場面</h3>
        {result.irritations.length === 0 ? (
          <p className="w8-muted" data-testid="irritations-none">嫌がられた場面はありませんでした。</p>
        ) : (
          <ul data-testid="irritations">
            {result.irritations.map((s) => (
              <li key={s.turn}>
                {s.turn} 番「{s.customer_text}」
                {s.prev_rep_turn !== null && <span className="w8-muted"> (直前のあなたの発言: {s.prev_rep_turn} 番「{turnText(s.prev_rep_turn)}」)</span>}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="w8-card cp-section">
        <h3 className="w8-section-title">質問の数</h3>
        <p data-testid="question-count">
          あなたの質問は {result.question_count} 問でした。{result.shortest_note}
        </p>
        <details>
          <summary>報告書の質問の数ごとの成約率を見る</summary>
          <table className="cp-table">
            <tbody>
              {result.question_bands.map((b) => (
                <tr key={b.band}>
                  <td>{b.band}</td>
                  <td>{b.rate_text}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="w8-muted">{result.reference_note}</p>
        </details>
      </section>

      <section className="w8-card cp-section">
        <h3 className="w8-section-title">見立て</h3>
        <p data-testid="heat-summary">
          あなたの見立て「{HEAT_LABEL[heat.guess] ?? heat.guess}」 / お客さんの型の熱さ「{HEAT_LABEL[heat.truth] ?? heat.truth}」
        </p>
        <p data-testid="heat-marks">
          隠れた答えにあった熱い印は {heat.hot_marks_hidden} 個、そのうち聞き出せたのは {heat.hot_marks_heard} 個でした。
        </p>
        <p data-testid="heat-message">{heat.message}</p>
      </section>

      <section className="w8-card cp-section">
        <h3 className="w8-section-title">良い例・直すとよい例</h3>
        {result.commentary === null ? (
          <p className="w8-muted" data-testid="commentary-note">{result.commentary_note ?? 'コメントはありません。'}</p>
        ) : (
          <div data-testid="commentary">
            {result.commentary.summary !== null && <p className="cp-summary">{result.commentary.summary}</p>}
            <h4>良かった発言</h4>
            {result.commentary.good.length === 0 ? <p className="w8-muted">挙げられた発言はありません。</p> : (
              <ul>
                {result.commentary.good.map((g) => (
                  <li key={`g${String(g.turn)}`}>
                    {g.turn} 番「{g.text}」 {g.comment}
                  </li>
                ))}
              </ul>
            )}
            <h4>直すとよい発言</h4>
            {result.commentary.bad.length === 0 ? <p className="w8-muted">挙げられた発言はありません。</p> : (
              <ul>
                {result.commentary.bad.map((b) => (
                  <li key={`b${String(b.turn)}`}>
                    {b.turn} 番「{b.text}」 {b.comment}
                    {b.better !== null && <div className="cp-better">言い直しの例: {b.better}</div>}
                  </li>
                ))}
              </ul>
            )}
            {result.commentary_note !== null && <p className="w8-muted">{result.commentary_note}</p>}
          </div>
        )}
      </section>

      <section className="w8-card cp-section" data-testid="persona-reveal">
        <h3 className="w8-section-title">お客さんの型</h3>
        <p>
          <strong data-testid="persona-label">{result.persona.label}</strong> / {result.persona.setting}
        </p>
        <ul>
          {result.persona.hidden.map((h) => (
            <li key={h.label}>
              {h.label}: {h.value}
            </li>
          ))}
        </ul>
        <p className="w8-muted">参考の成約率(報告書): {result.persona.reference}</p>
        <p className="w8-muted">{result.reference_note}</p>
        {result.script_seen && <p className="w8-muted">この練習は台本を横に出して行いました。</p>}
      </section>

      <div className="cp-actions">
        <button type="button" className="w8-button" onClick={onAgainSame}>
          同じ型でもう一度
        </button>
        <button type="button" className="w8-button" onClick={onAgainOther}>
          別の型で練習する
        </button>
      </div>
    </div>
  );
}
