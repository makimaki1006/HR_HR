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
