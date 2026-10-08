export type CopyComparisonStatus = 'initial' | 'unchanged' | 'format_only' | 'changed' | 'unavailable';
export interface CopyDiffLine { kind: 'same' | 'added' | 'removed'; text: string }
export interface CopyComparison { status: CopyComparisonStatus; lines: CopyDiffLine[] }

const MAX_LCS_CELLS = 1_000_000;

function at<T>(values: ArrayLike<T>, index: number): T {
  const value = values[index];
  if (value === undefined) throw new RangeError('Diff index is outside the input.');
  return value;
}

// Splitting only on LF keeps the original CR and trailing empty line visible in
// the raw comparison. Only the status calculation normalizes CRLF.
function rawLines(text: string): string[] {
  return text.split('\n');
}

function compareLines(before: string[], after: string[]): CopyDiffLine[] {
  const lines: CopyDiffLine[] = [];
  let start = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) {
    lines.push({ kind: 'same', text: at(before, start) });
    start += 1;
  }
  let oldEnd = before.length;
  let newEnd = after.length;
  while (oldEnd > start && newEnd > start && before[oldEnd - 1] === after[newEnd - 1]) {
    oldEnd -= 1;
    newEnd -= 1;
  }

  const oldLength = oldEnd - start;
  const newLength = newEnd - start;
  if ((oldLength + 1) * (newLength + 1) > MAX_LCS_CELLS) {
    // Large changed regions are shown as a whole replacement. This preserves
    // every source line while bounding CPU and memory rather than truncating.
    for (let i = start; i < oldEnd; i += 1) lines.push({ kind: 'removed', text: at(before, i) });
    for (let j = start; j < newEnd; j += 1) lines.push({ kind: 'added', text: at(after, j) });
  } else {
    const width = newLength + 1;
    const lengths = new Uint32Array((oldLength + 1) * width);
    for (let i = oldLength - 1; i >= 0; i -= 1) {
      for (let j = newLength - 1; j >= 0; j -= 1) {
        lengths[i * width + j] = before[start + i] === after[start + j]
          ? at(lengths, (i + 1) * width + j + 1) + 1
          : Math.max(at(lengths, (i + 1) * width + j), at(lengths, i * width + j + 1));
      }
    }
    let i = 0;
    let j = 0;
    while (i < oldLength || j < newLength) {
      if (i < oldLength && j < newLength && before[start + i] === after[start + j]) {
        lines.push({ kind: 'same', text: at(before, start + i) });
        i += 1;
        j += 1;
      } else if (i < oldLength && (j === newLength || at(lengths, (i + 1) * width + j) >= at(lengths, i * width + j + 1))) {
        lines.push({ kind: 'removed', text: at(before, start + i) });
        i += 1;
      } else {
        lines.push({ kind: 'added', text: at(after, start + j) });
        j += 1;
      }
    }
  }
  for (let i = oldEnd; i < before.length; i += 1) lines.push({ kind: 'same', text: at(before, i) });
  return lines;
}

/** Compare received text without claiming that the publisher updated a listing. */
export function compareCopy(before: string | null, after: string | null): CopyComparison {
  if (after === null || after.trim() === '') return { status: 'unavailable', lines: [] };
  if (before === null || before.trim() === '') {
    return { status: 'initial', lines: rawLines(after).map((text) => ({ kind: 'added', text })) };
  }
  if (before === after) {
    return { status: 'unchanged', lines: rawLines(after).map((text) => ({ kind: 'same', text })) };
  }
  const status = before.replace(/\r\n/g, '\n') === after.replace(/\r\n/g, '\n') ? 'format_only' : 'changed';
  return { status, lines: compareLines(rawLines(before), rawLines(after)) };
}

/** A piece of a changed line: changed pieces are the words or characters that differ from the paired line. */
export interface InlineSegment { text: string; changed: boolean }
export interface MarkedDiffLine extends CopyDiffLine {
  /** Set on a removed or added line paired with a line on the other side; absent on unpaired lines. */
  segments?: InlineSegment[];
}

/** Bound on token pairs compared within one line pair; longer pairs keep the whole-line colour only. */
const MAX_INLINE_CELLS = 250_000;

/** Runs of digits (with , and .) and of Latin letters stay whole, so 250,000 → 270,000 marks the number. */
function tokens(text: string): string[] {
  return text.match(/[0-9０-９][0-9０-９,，.．]*|[A-Za-zＡ-Ｚａ-ｚ]+|\s+|[\s\S]/gu) ?? [];
}

function inlineSegments(before: string, after: string): [InlineSegment[], InlineSegment[]] | null {
  const a = tokens(before); const b = tokens(after);
  if ((a.length + 1) * (b.length + 1) > MAX_INLINE_CELLS) return null;
  const width = b.length + 1;
  const lengths = new Uint32Array((a.length + 1) * width);
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      lengths[i * width + j] = a[i] === b[j] ? at(lengths, (i + 1) * width + j + 1) + 1 : Math.max(at(lengths, (i + 1) * width + j), at(lengths, i * width + j + 1));
    }
  }
  const left: InlineSegment[] = []; const right: InlineSegment[] = [];
  const push = (list: InlineSegment[], text: string, changed: boolean) => {
    const last = list[list.length - 1];
    if (last?.changed === changed) last.text += text; else list.push({ text, changed });
  };
  let i = 0; let j = 0;
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) { push(left, at(a, i), false); push(right, at(b, j), false); i += 1; j += 1; }
    else if (i < a.length && (j === b.length || at(lengths, (i + 1) * width + j) >= at(lengths, i * width + j + 1))) { push(left, at(a, i), true); i += 1; }
    else { push(right, at(b, j), true); j += 1; }
  }
  return [readable(left), readable(right)];
}

/**
 * A single unchanged character between two changed pieces (「の」 in いつもの道 → 自分の時間) splits
 * one change into fragments; it is shown as part of the change.
 */
function readable(segments: InlineSegment[]): InlineSegment[] {
  const merged: InlineSegment[] = [];
  segments.forEach((segment, index) => {
    const bridge = !segment.changed && Array.from(segment.text).length === 1 && segments[index - 1]?.changed === true && segments[index + 1]?.changed === true;
    const changed = segment.changed || bridge;
    const last = merged[merged.length - 1];
    if (last?.changed === changed) last.text += segment.text; else merged.push({ text: segment.text, changed });
  });
  return merged;
}

/**
 * Pairs each block of removed lines with the added lines that follow it (first with first, and so
 * on) and marks the words or characters that differ inside each pair. Lines without a partner, and
 * pairs that share nothing, keep the whole-line colour only.
 */
export function markInlineChanges(lines: readonly CopyDiffLine[]): MarkedDiffLine[] {
  const result: MarkedDiffLine[] = lines.map(line => ({ ...line }));
  let index = 0;
  while (index < result.length) {
    if (result[index]?.kind !== 'removed') { index += 1; continue; }
    const removedStart = index;
    while (result[index]?.kind === 'removed') index += 1;
    const addedStart = index;
    while (result[index]?.kind === 'added') index += 1;
    const pairs = Math.min(addedStart - removedStart, index - addedStart);
    for (let k = 0; k < pairs; k += 1) {
      const removed = result[removedStart + k]; const added = result[addedStart + k];
      if (!removed || !added) continue;
      const marked = inlineSegments(removed.text, added.text);
      if (!marked?.[0].some(segment => !segment.changed && segment.text.trim())) continue;
      removed.segments = marked[0]; added.segments = marked[1];
    }
  }
  return result;
}
