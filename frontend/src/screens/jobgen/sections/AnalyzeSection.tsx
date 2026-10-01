// ② 市場分析 (旧 renderAnalyze)。
import type { Analysis } from '../../../generated/Analysis';
import { ConfirmBox, GateBadge, SectionHead } from '../parts';
import type { StepKey } from '../state';

function Col({ title, items }: { title: string; items: string[] }) {
  return (
    <div className="col-card">
      <h3>{title}</h3>
      <ul>{items.length ? items.map((x, i) => <li key={i}>{x}</li>) : <li>—</li>}</ul>
    </div>
  );
}

export function AnalyzeSection({
  analysis,
  category,
  knowledgeUsed,
  confirmed,
  onConfirm,
}: {
  analysis: Analysis;
  category: string;
  knowledgeUsed: boolean;
  confirmed: boolean;
  onConfirm: (key: StepKey, checked: boolean) => void;
}) {
  const detail = category
    ? `${category}${knowledgeUsed ? 'の知識を使用' : '（該当知識なし・汎用）'}`
    : '—';
  return (
    <>
      <SectionHead
        num="②"
        name="市場分析"
        gates={<GateBadge label="職種知識注入" cls={knowledgeUsed ? 'ok' : 'warn'} detail={detail} />}
      />
      {!knowledgeUsed && (
        <div className="knowledge-warn">
          ⚠
          この職種の専門知識が見つからなかったため、汎用的な内容で分析しています。職種名を「カフェスタッフ」のような一般的な呼び方に直し、上の職種名欄を修正して
          <b>工程②だけ再実行</b>すると精度が上がります。
        </div>
      )}
      <div className="cols3">
        <Col title="表面の強み" items={analysis.surface_strengths} />
        <Col title="裏の強み" items={analysis.hidden_strengths} />
        <Col title="ボトルネック" items={analysis.bottlenecks} />
      </div>
      <ConfirmBox stepKey="analyze" checked={confirmed} onChange={onConfirm} />
    </>
  );
}
