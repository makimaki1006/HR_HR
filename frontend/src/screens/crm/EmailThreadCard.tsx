import { memo, useId, useState } from 'react';
import type { EmailThread } from './emailThreads';
import { ActivityCard } from './ActivityCard';
import './email-thread.css';
function EmailThreadCardImpl({ thread, ownerNames, highlightedId }: { thread: EmailThread; ownerNames: ReadonlyMap<string, string>; highlightedId: string | null }) {
  const [open, setOpen] = useState(false);
  const [order, setOrder] = useState<'newest' | 'oldest'>('newest');
  const listId = useId();
  const latest = thread.messages[0];
  if (!latest) return null;
  const shown = open ? (order === 'newest' ? thread.messages : [...thread.messages].reverse()) : [latest];
  const highlighted = thread.messages.some(a => a.id === highlightedId);
  return <li className={`wd-email-thread${highlighted ? ' crm-email-highlight' : ''}`} data-testid="email-thread">
    <div className="wd-thread-head">
      <button type="button" className="wd-thread-toggle" aria-expanded={open} aria-controls={listId} onClick={() => { setOpen(v => !v); }}>
        <span aria-hidden="true">{open ? '▾' : '▸'}</span> やり取り {thread.messages.length} 件{open ? 'を閉じる' : 'を開く'}
      </button>
      {open && <label>並び順 <select value={order} onChange={e => { setOrder(e.target.value as 'newest' | 'oldest'); }}>
        <option value="newest">新しい順</option><option value="oldest">古い順</option>
      </select></label>}
    </div>
    <ul id={listId} className="wd-thread-messages">{shown.map(a => <ActivityCard key={a.id} a={a} ownerNames={ownerNames} highlighted={a.id === highlightedId} />)}</ul>
  </li>;
}
export const EmailThreadCard = memo(EmailThreadCardImpl);
