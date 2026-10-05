// 営業KPI: HubSpot 直読みの状態表示 (画面上部)。直読みが無効 (シート) のときは何も出さない。
// 旧画面の #hsstatus と同じ DOM・文言 (directStatus.ts が両方の正本)。
import { directStatus } from './directStatus';

export function DirectStatusBanner({
  meta,
  exhausted,
}: {
  meta: Record<string, string> | null | undefined;
  exhausted: boolean;
}) {
  const s = directStatus(meta, exhausted);
  if (s === null) return null;
  return (
    <div className={'hsstatus ' + s.kind} id="hsstatus" role="status">
      <div className="hs-head">{s.head}</div>
      {s.sub !== null ? <div className="hs-sub">{s.sub}</div> : null}
      <div className="hs-note">{s.note}</div>
    </div>
  );
}
