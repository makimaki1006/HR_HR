"""Hellowork-deploy Claude Code hooks: 共通ユーティリティ。

各 hook スクリプトはこの module を使って transcript 解析と判定を行う。
標準ライブラリのみ使用 (Windows / git-bash 環境で追加 install 不要)。
"""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

# Windows の default stdout/stderr が cp932 だと日本語含む block reason / JSON 出力が
# UnicodeEncodeError で silent fail する。UTF-8 を強制して hook を確実に動作させる。
try:
    sys.stdout.reconfigure(encoding="utf-8")  # type: ignore[attr-defined]
    sys.stderr.reconfigure(encoding="utf-8")  # type: ignore[attr-defined]
except Exception:
    pass

PROJECT_MARKERS = ["Cargo.toml", "src/handlers/survey/upload.rs"]


def read_input_json() -> dict:
    """stdin から hook の入力 JSON を読む。失敗時は {}。"""
    try:
        raw = sys.stdin.read()
    except Exception:
        return {}
    if not raw.strip():
        return {}
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return {}


def is_in_project(payload: dict) -> bool:
    """このプロジェクト (hellowork-deploy) で動いているかを判定。

    cwd 直下に Cargo.toml と handlers/survey/upload.rs がある場合のみ True。
    別プロジェクトでは hook を no-op にしてスコープ外動作を防ぐ。
    """
    cwd = payload.get("cwd") or os.getcwd()
    cwd_p = Path(cwd)
    return all((cwd_p / m).exists() for m in PROJECT_MARKERS)


def read_transcript(path: str | None) -> list[dict]:
    """transcript.jsonl を読んで message dict のリストを返す。"""
    if not path:
        return []
    p = Path(path)
    if not p.exists():
        return []
    msgs: list[dict] = []
    try:
        with open(p, "r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    msgs.append(json.loads(line))
                except json.JSONDecodeError:
                    pass
    except OSError:
        return []
    return msgs


def _msg_role(msg: dict) -> str:
    """message dict から role / type を解決。"""
    return msg.get("type") or msg.get("role") or ""


def _msg_content(msg: dict):
    """message dict から content (str | list) を取り出す。"""
    inner = msg.get("message")
    if isinstance(inner, dict):
        c = inner.get("content")
        if c is not None:
            return c
    return msg.get("content", "")


def _is_tool_result_msg(msg: dict) -> bool:
    """user message でも content が tool_result のみなら「ユーザー発言」ではない。"""
    if _msg_role(msg) != "user":
        return False
    content = _msg_content(msg)
    if not isinstance(content, list):
        return False
    return all(
        isinstance(c, dict) and c.get("type") == "tool_result"
        for c in content
    ) and len(content) > 0


def _extract_text(msg: dict) -> str:
    """message から text のみ抽出。tool_use / tool_result の text 部分も拾う。"""
    content = _msg_content(msg)
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts = []
        for c in content:
            if isinstance(c, dict):
                if c.get("type") == "text" or "text" in c:
                    t = c.get("text")
                    if t:
                        parts.append(str(t))
        return "\n".join(parts)
    return ""


def get_assistant_last_text(transcript: list[dict]) -> str:
    """直近 1 件の assistant message の text を返す。"""
    for msg in reversed(transcript):
        if _msg_role(msg) == "assistant":
            text = _extract_text(msg)
            if text:
                return text
    return ""


def get_user_recent_prompts(transcript: list[dict], n: int = 5) -> list[str]:
    """直近 n 件のユーザー発言 (新しい順)。tool_result は除外。"""
    out: list[str] = []
    for msg in reversed(transcript):
        if _msg_role(msg) != "user":
            continue
        if _is_tool_result_msg(msg):
            continue
        text = _extract_text(msg)
        if text:
            out.append(text)
            if len(out) >= n:
                break
    return out


def get_recent_tool_uses(transcript: list[dict], n: int = 30) -> list[dict]:
    """直近 n 件の assistant message に含まれる tool_use を全取得 (新しい順)。"""
    out: list[dict] = []
    turns = 0
    for msg in reversed(transcript):
        if _msg_role(msg) != "assistant":
            continue
        content = _msg_content(msg)
        if isinstance(content, list):
            for c in content:
                if isinstance(c, dict) and c.get("type") == "tool_use":
                    out.append(c)
        turns += 1
        if turns >= n:
            break
    return out


def get_recent_tool_results(transcript: list[dict], n: int = 30) -> list[str]:
    """直近 n 件分の tool_result text を返す。"""
    out: list[str] = []
    turns = 0
    for msg in reversed(transcript):
        if _msg_role(msg) != "user":
            continue
        content = _msg_content(msg)
        if not isinstance(content, list):
            continue
        captured = False
        for c in content:
            if isinstance(c, dict) and c.get("type") == "tool_result":
                rc = c.get("content")
                if isinstance(rc, str):
                    out.append(rc)
                    captured = True
                elif isinstance(rc, list):
                    for r in rc:
                        if isinstance(r, dict):
                            t = r.get("text")
                            if t:
                                out.append(str(t))
                                captured = True
        if captured:
            turns += 1
            if turns >= n:
                break
    return out


BYPASS_RE = re.compile(
    r"(テスト不要|テスト無しで|テストなしで|テストスキップ"
    r"|強制\s*push|force[-_\s]*push"
    r"|hooks?\s*off|フック\s*停止|フック\s*無視|hook\s*無効)",
    re.IGNORECASE,
)


def has_bypass_signal(transcript: list[dict], n: int = 5) -> bool:
    """ユーザーが直近 n 発言で hook bypass を明言しているか。"""
    for prompt in get_user_recent_prompts(transcript, n):
        if BYPASS_RE.search(prompt):
            return True
    return False


def block(message: str) -> None:
    """応答を block する (exit 2 + stderr メッセージで Claude に reason を返す)。"""
    sys.stderr.write(message + "\n")
    sys.exit(2)


def pass_through() -> None:
    """no-op で抜ける。"""
    sys.exit(0)
