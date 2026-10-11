import type { WorkspaceActivity } from '../../generated/WorkspaceActivity';
export interface EmailThread { kind: 'thread'; key: string; messages: WorkspaceActivity[] }
export type ActivityEntry = EmailThread | { kind: 'activity'; key: string; activity: WorkspaceActivity };
export function normalizedEmailSubject(subject: string | null): string {
  return (subject ?? '').normalize('NFKC').trim().replace(/^(?:(?:re|fw|fwd)\s*(?:\[\d+\])?\s*[:：]\s*)+/iu, '').trim().replace(/\s+/gu, ' ').toLowerCase();
}
function address(raw: string): string { return (/<([^<>]+)>/u.exec(raw)?.[1] ?? raw).normalize('NFKC').trim().toLowerCase(); }
function correspondent(a: WorkspaceActivity): string | null {
  const direction = a.direction?.trim().toUpperCase();
  const people = direction === 'INCOMING_EMAIL' ? [a.rich?.from_email ?? '']
    : direction === 'EMAIL' || direction === 'FORWARDED_EMAIL' ? a.rich?.to ?? [] : [];
  const addresses = [...new Set(people.map(address).filter(p => /^[^\s@]+@[^\s@]+$/u.test(p)))].sort();
  return addresses.length > 0 ? JSON.stringify(addresses) : null;
}
export function emailThreadKey(a: WorkspaceActivity): string | null {
  if (a.kind !== 'email') return null;
  const id = a.rich?.thread_id?.trim();
  if (id) return `thread:${id}`;
  const subject = normalizedEmailSubject(a.title);
  const peer = correspondent(a);
  return subject && peer ? `fallback:${JSON.stringify([subject, peer])}` : null;
}
function time(a: WorkspaceActivity | undefined): number {
  const raw = a?.timestamp ?? '';
  const n = /^\d+$/u.test(raw) ? Number(raw) : Date.parse(raw);
  return Number.isFinite(n) ? n : 0;
}
/** Returns new arrays, never sorts the server's activity data in place. */
export function groupActivityEmails(activities: readonly WorkspaceActivity[]): ActivityEntry[] {
  const entries: ActivityEntry[] = [];
  const threads = new Map<string, EmailThread>();
  for (const activity of activities) {
    const key = emailThreadKey(activity);
    if (key === null) { entries.push({ kind: 'activity', key: `${activity.kind}:${activity.id}`, activity }); continue; }
    const found = threads.get(key);
    if (found) found.messages.push(activity);
    else { const thread: EmailThread = { kind: 'thread', key, messages: [activity] }; threads.set(key, thread); entries.push(thread); }
  }
  for (const entry of entries) if (entry.kind === 'thread') entry.messages.sort((a, b) => time(b) - time(a));
  return entries.sort((a, b) => time(b.kind === 'thread' ? b.messages[0] : b.activity) - time(a.kind === 'thread' ? a.messages[0] : a.activity));
}
