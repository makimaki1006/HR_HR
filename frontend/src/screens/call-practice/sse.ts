// サーバが流す SSE (event: / data: の組が空行で区切られる) を読む。

export interface SseEvent {
  event: string;
  data: unknown;
}

/** 受け取った文字を貯めた buffer から、完結したイベントを取り出す。残りは次の呼び出しに持ち越す */
export function parseSse(buffer: string): { events: SseEvent[]; rest: string } {
  const normalized = buffer.replace(/\r\n/g, '\n');
  const parts = normalized.split('\n\n');
  const rest = parts.pop() ?? '';
  const events: SseEvent[] = [];
  for (const block of parts) {
    let event = 'message';
    const dataLines: string[] = [];
    for (const line of block.split('\n')) {
      if (line.startsWith('event:')) event = line.slice(6).trim();
      else if (line.startsWith('data:')) dataLines.push(line.slice(5).trim());
    }
    if (dataLines.length === 0) continue;
    try {
      events.push({ event, data: JSON.parse(dataLines.join('\n')) as unknown });
    } catch {
      // 壊れたイベントは読み飛ばす
    }
  }
  return { events, rest };
}
