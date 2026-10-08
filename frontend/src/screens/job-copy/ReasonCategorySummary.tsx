import type { ApplicantReasonCollection } from './applicantReasonsModel';
import { reasonSourceLabels } from './applicantReasonsModel';
import { EXCLUDED_PHRASE_NOTES, REASON_CATEGORIES, REASON_KEYWORDS, classifyApplicationReasons, classifyTransferReasons, shareText, tally } from './reasonCategories';
import type { Classification } from './reasonCategories';
import { formatDateJst } from './format';

/** Per source: recorded / blank / not recorded, or 未取得 when the source was not read (never 0). */
function SourceCounts({ collection }: { collection: ApplicantReasonCollection }) {
  return <div className="ar-table-scroll" role="region" aria-label="記録欄ごとの件数" tabIndex={0}><table className="ar-table">
    <thead><tr><th scope="col">記録欄</th><th scope="col">記入あり</th><th scope="col">空欄</th><th scope="col">記録なし</th></tr></thead>
    <tbody>{Object.keys(reasonSourceLabels).map(property => {
      const counts = collection.sourceCounts[property];
      return <tr key={property}><th scope="row">{reasonSourceLabels[property]}</th>
        {counts ? <><td>{counts.nonblank}件</td><td>{counts.blank}件</td><td>{counts.missing}件</td></> : <td colSpan={3}>未取得（0件という意味ではありません）</td>}
      </tr>;
    })}</tbody>
  </table></div>;
}

function CategoryTable({ label, classification, withSelected }: { label: string; classification: Classification; withSelected: boolean }) {
  const result = tally(classification.applications);
  const unit = classification.unit === 'text' ? '記述' : '応募';
  return <>
    <p>n={result.n}（{unit}{result.n}件）{withSelected && <> · 選択済み{result.selectedN}件</>} · キーワードで推定{result.estimatedN}件 · 分類できない{result.unclassified}件</p>
    {result.n === 0 ? <p>分類できる記録はありません。</p> : <div className="ar-table-scroll" role="region" aria-label={label} tabIndex={0}><table className="ar-table">
      <thead><tr><th scope="col">分類</th><th scope="col">合計</th>{withSelected && <th scope="col">選択済み</th>}<th scope="col">キーワードで推定</th><th scope="col">nに対する割合</th></tr></thead>
      <tbody>{result.counts.map(row => <tr key={row.category}><th scope="row">{row.category}</th><td>{row.total}件</td>{withSelected && <td>{row.selected}件</td>}<td>{row.estimated}件</td><td>{shareText(row.total, result.n) ?? `n=${String(result.n)}のため出しません`}</td></tr>)}</tbody>
    </table></div>}
    {result.n > 0 && result.n < 5 && <p className="ar-note">nが5件に満たないため、割合は出さず件数だけを示します。</p>}
  </>;
}

function Unclassified({ classification }: { classification: Classification }) {
  const rows = classification.applications.filter(application => application.basis === 'unclassified');
  if (!rows.length) return null;
  return <details className="ar-unclassified"><summary>分類できなかった記録を開く（{rows.length}件・社内確認用）</summary>
    <p>住所・電話番号・メールアドレス・「さん」「様」の付いた名前は、読み取れた範囲で「＊＊」に置き換えています。それ以外の個人情報が残っていることがあります。</p>
    <ol className="ar-texts">{rows.map(row => <li key={row.key}>
      <p>応募日: {row.applicationDate ? formatDateJst(row.applicationDate, row.applicationDate) : '不明'}</p>
      {row.otherValues.length > 0 && <p>選ばれた分類: {row.otherValues.join('・')}（決まった分類のどれにも当たりません）</p>}
      {row.unnamedSelections > 0 && <p>分類は選ばれていますが、分類の名前を読み取れませんでした（選択済みには数えていません）。</p>}
      {row.texts.map(text => <blockquote key={text.id}>{text.text}</blockquote>)}
    </li>)}</ol>
  </details>;
}

export function ReasonCategorySummary({ collection }: { collection: ApplicantReasonCollection }) {
  const reasons = classifyApplicationReasons(collection);
  const transfer = classifyTransferReasons(collection);
  return <section className="ar-categories" aria-label="応募理由の分類">
    <h3>記録欄ごとの件数</h3>
    <SourceCounts collection={collection} />
    <h3>応募理由の分類</h3>
    <p>HubSpotで分類が選ばれた応募は「選択済み」、分類が選ばれていない応募は応募動機・応募理由の文から言葉で分類した「キーワードで推定」として、分けて数えます。1件の応募が複数の分類に入ることがあります。</p>
    {collection.selections === null && <p className="jc-notice">この取得データには「応募理由の分類」の記録欄が含まれていません（未取得）。選択済みの件数は0件ではなく不明です。</p>}
    {reasons && <CategoryTable label="応募理由の分類の件数" classification={reasons} withSelected />}
    {reasons && reasons.unnamedApplications > 0 && <p className="jc-notice">分類が選ばれているのに分類の名前を読み取れなかった応募が {reasons.unnamedApplications}件 あります。この応募は選択済みに数えず、文があれば言葉で推定しています。時間をおいて開き直すと読み取れることがあります。</p>}
    {reasons && reasons.unsetOnly > 0 && (collection.truncated
      ? <p>分類が「未設定」で、読み込めた文もない応募 {reasons.unsetOnly}件 は数えていません（記述が上限を超えて一部を読み込んでいないため、文が記録されている応募も含まれることがあります）。</p>
      : <p>分類が「未設定」で文もない応募 {reasons.unsetOnly}件 は数えていません。</p>)}
    {reasons?.unit === 'text' && <p className="jc-notice">この取得データでは同じ応募の記述を見分けられないため、記述ごとに数えています。1件の応募が複数回数えられていることがあります。</p>}
    {collection.truncated && <p className="jc-notice">表示できる記述の上限を超えたため、一部の記述は分類に入っていません。</p>}
    {reasons && <Unclassified classification={reasons} />}
    <h3>今の仕事・前の仕事から転職する理由の分類</h3>
    <p>応募理由とは別に、文から言葉で分類しています（すべて「キーワードで推定」）。</p>
    {transfer ? <><CategoryTable label="転職理由の分類の件数" classification={transfer} withSelected={false} /><Unclassified classification={transfer} /></> : <p>未取得です（0件という意味ではありません）。</p>}
    <details className="ar-keywords"><summary>分類に使う言葉の一覧</summary>
      <p>文にこれらの言葉が含まれると、その分類に入れます。言葉が含まれるかどうかだけで分けるため、読み違えることがあります。</p>
      <dl>{REASON_CATEGORIES.map(category => <div key={category}><dt>{category}</dt><dd>{REASON_KEYWORDS[category].join('、')}</dd></div>)}</dl>
      <p>次の言い回しの中の言葉は、別の意味になるため分類に使いません: {EXCLUDED_PHRASE_NOTES.join('、')}。</p>
    </details>
  </section>;
}
