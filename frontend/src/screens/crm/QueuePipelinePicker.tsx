import { useRef, useState } from 'react';
import { useDismiss } from './useDismiss';
import { pipelineName } from './queuePipelines';
import type { QueuePipelineDef } from './queuePipelines';
export function QueuePipelinePicker({ pipelines, selected, onApply }: { pipelines: readonly QueuePipelineDef[]; selected: string[]; onApply: (ids:string[])=>void }) {
  const [draft,setDraft]=useState<Set<string>|null>(null);
  const root=useRef<HTMLDivElement>(null);const toggle=useRef<HTMLButtonElement>(null);
  useDismiss(root,draft!==null,()=>{setDraft(null);},()=>{setDraft(null);toggle.current?.focus();});
  const chosen=pipelines.filter(p=>selected.includes(p.id));
  return <div className="cq-pipeline-picker" ref={root}><button type="button" className="cq-pipeline cq-btn" ref={toggle} aria-label="パイプライン" aria-expanded={draft!==null} onClick={()=>{setDraft(draft?null:new Set(selected));}}>
    {chosen[0]?pipelineName(chosen[0]):'パイプラインを選択'}{chosen.length>1?` ほか ${String(chosen.length-1)}`:''}</button>
    {draft&&<section className="cq-pipeline-options" aria-label="パイプラインの選択"><p>1つ以上選んでください</p>{pipelines.map(p=><label key={p.id}><input type="checkbox" checked={draft.has(p.id)} disabled={draft.size===1&&draft.has(p.id)} onChange={e=>{setDraft(prev=>{const next=new Set(prev);if(e.target.checked)next.add(p.id);else next.delete(p.id);return next;});}} />{pipelineName(p)}</label>)}
      <div><button type="button" className="cq-btn" disabled={draft.size===0} onClick={()=>{onApply([...draft]);setDraft(null);}}>パイプラインを適用</button><button type="button" className="cq-btn" onClick={()=>{setDraft(null);}}>閉じる</button></div>
    </section>}
  </div>;
}
