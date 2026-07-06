#!/usr/bin/env python3
"""
lint_statistical_claims.py
==========================
Checks Japanese string literals in Rust source files under
  src/handlers/survey/report_html/**/*.rs
for forbidden statistical-expression patterns.

Exit codes:
  0  No violations found
  1  One or more violations found
  2  Target directory not found (configuration error)

Allow-list
----------
Add the comment  // lint-allow: statistical-claim  anywhere on the same
source line as the pattern to suppress that finding.
Do not over-rely on this escape hatch; fix the root cause first.

Forbidden pattern categories
-----------------------------
1. Effect promises  : conditional + HR-outcome in affirmative form
                      (suRUTO / surukoto-de + "improves/increases" etc.)
2. Causal assertions: "原因は〜です" / "因果関係があります" etc.
3. Hyperbolic words : 劇的、完璧 (combined with specific outcome words)
4. Absolute + outcome: 必ず + (向上/改善/…)する
5. Assertive labels : 離職多発 (without an immediate denial modifier),
                      流出継続

Dependencies: Python 3.8+ stdlib only.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Resolve target directory relative to this script's location.
# Script is at  <repo_root>/scripts/lint_statistical_claims.py
# Target is     <repo_root>/src/handlers/survey/report_html/
_SCRIPT_DIR = Path(__file__).resolve().parent
TARGET_DIR = _SCRIPT_DIR.parent / "src" / "handlers" / "survey" / "report_html"

ALLOW_MARKER = "lint-allow: statistical-claim"

# ---------------------------------------------------------------------------
# Forbidden patterns
# ---------------------------------------------------------------------------
# Each entry: (compiled regex, human-readable description)
#
# Design principles:
#   - High precision over high recall: never fire on currently correct code.
#   - Patterns detect *affirmative* causal/absolute claims, not their negations.
#   - Negative lookaheads exclude known-good denial suffixes.
# ---------------------------------------------------------------------------

FORBIDDEN: list[tuple[re.Pattern[str], str]] = [
    # ------------------------------------------------------------------
    # 1. Effect promises: conditional trigger + HR outcome (affirmative)
    # ------------------------------------------------------------------
    (
        re.compile(
            r"(?:ことで|すると|により|ば)"
            r".{0,20}"
            r"(?:向上します|改善されます|上昇します|増加します|高まります"
            r"|向上する|改善される|上昇する|増加する|高まる)"
        ),
        "効果約束: 条件節+HR成果の断定 (ことで/すると/により/ば + 向上/改善等)",
    ),
    # ------------------------------------------------------------------
    # 2. Causal assertions
    # ------------------------------------------------------------------
    (
        re.compile(r"原因は.{0,30}(?:です。|にあります|だから|のためです)"),
        "因果断定: 「原因は〜です」肯定文",
    ),
    (
        re.compile(
            r"因果(?:関係)?"
            r"(?:があります|があることを|を証明しました|を証明できます|が確認されました)"
        ),
        "因果断定: 「因果関係があります/を証明しました」等の肯定",
    ),
    # ------------------------------------------------------------------
    # 3. Hyperbolic language (combined with specific outcome nouns)
    # ------------------------------------------------------------------
    (
        re.compile(r"劇的(?:に|な)(?:改善|向上|増加|減少|変化|効果)"),
        "誇張語: 「劇的な/劇的に」+成果語",
    ),
    (
        re.compile(r"完璧(?:に|な)(?:解決|改善|達成|成功)"),
        "誇張語: 「完璧に/完璧な」+成果語",
    ),
    # ------------------------------------------------------------------
    # 4. Absolute guarantee + outcome
    # ------------------------------------------------------------------
    (
        re.compile(
            r"必ず(?:向上|改善|上昇|増加|高まり|成功|達成)"
            r"(?:します|する|される)"
        ),
        "絶対表現: 「必ず」+確定的な成果 (向上します/改善されます等)",
    ),
    # ------------------------------------------------------------------
    # 5. Assertive labels — re-introduction prevention
    # ------------------------------------------------------------------
    (
        re.compile(r"離職多発(?!だけでなく|ではなく|とは限らない|のみが|に限ら)"),
        "断定ラベル: [離職多発](直後に否定修飾なし) -- 免責文脈でのみ使用可",
    ),
    (
        re.compile(r"流出継続"),
        "断定ラベル: [流出継続] -- 使用禁止",
    ),
]


# ---------------------------------------------------------------------------
# Core checker
# ---------------------------------------------------------------------------

def check_file(path: Path) -> list[tuple[int, str, str]]:
    """
    Return list of (line_number, matched_text, description) for violations.

    Lines skipped:
    - Pure Rust line-comment lines (stripped content starts with ``//`` or ``///``)
    - Lines containing the allow-list marker
    """
    violations: list[tuple[int, str, str]] = []

    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        # Rare: skip files that cannot be decoded as UTF-8
        return []
    except OSError as exc:
        print(f"WARNING: cannot read {path}: {exc}", file=sys.stderr)
        return []

    for lineno, line in enumerate(text.splitlines(), start=1):
        stripped = line.strip()

        # Skip pure Rust comment lines
        if stripped.startswith("//"):
            continue

        # Skip lines with the allow-list marker
        if ALLOW_MARKER in line:
            continue

        for pattern, description in FORBIDDEN:
            match = pattern.search(line)
            if match:
                violations.append((lineno, match.group(0), description))
                # Report only the first violation per line per pattern pass
                # (a single bad line may trigger multiple patterns, all are reported)

    return violations


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> int:
    if not TARGET_DIR.exists():
        print(
            f"ERROR: Target directory not found: {TARGET_DIR}\n"
            "       Is the script placed at <repo_root>/scripts/?",
            file=sys.stderr,
        )
        return 2

    rs_files = sorted(TARGET_DIR.rglob("*.rs"))
    if not rs_files:
        print("WARNING: No *.rs files found under target directory.", file=sys.stderr)
        return 0

    total_violations = 0

    for path in rs_files:
        violations = check_file(path)
        for lineno, matched_text, description in violations:
            # Print in a format compatible with most CI log parsers:
            # <file>:<line>: [<rule>] matched: "<text>"
            print(f"{path}:{lineno}: [{description}] matched: {repr(matched_text)}")
            total_violations += 1

    if total_violations == 0:
        print(
            f"lint_statistical_claims: OK -- 0 violations in {len(rs_files)} file(s)."
        )
        return 0

    print(
        f"\nlint_statistical_claims: FAIL -- {total_violations} violation(s) found "
        f"in {len(rs_files)} file(s)."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
