#!/usr/bin/env python3
"""Docker builder の COPY 入力だけで Rust の本番用コンパイルを検査する。

RUN は実行しない（依存キャッシュ用のダミーソースも不要）。現在の builder
には COPY --from が無い。追加時は成果物を用意する実装が必要なので拒否する。
テスト専用 include は cargo check --release --locked の対象に含めない。
"""

import argparse
import json
import os
from pathlib import Path, PurePosixPath
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time


class ContextError(ValueError):
    pass


def literal(value):
    if any(c in value for c in '$*?[]\\') or value.startswith(('http:', 'https:')):
        raise ContextError(f'未対応の COPY/WORKDIR パス: {value}（展開・ワイルドカード等）')
    return value


def image_path(value, workdir):
    value = literal(value)
    path = PurePosixPath(value) if value.startswith('/') else workdir / value
    if '..' in path.parts:
        raise ContextError(f'上位参照には未対応: {value}')
    return path


def instructions(dockerfile):
    pending = ''
    for line in dockerfile.read_text().splitlines():
        if line.lstrip().startswith('#'):
            if re.match(r'#\s*escape\s*=', line, re.I):
                raise ContextError('Dockerfile の escape 指示には未対応')
            continue
        line = line.strip()
        if not line:
            continue
        pending += line[:-1] + ' ' if line.endswith('\\') else line
        if line.endswith('\\'):
            continue
        fields = pending.split(None, 1)
        name, body = fields[0], fields[1] if len(fields) == 2 else ''
        if '<<' in body:
            raise ContextError('Dockerfile の heredoc は入力再現に未対応です')
        yield name.upper(), body.strip()
        pending = ''
    if pending:
        raise ContextError('Dockerfile の行継続が閉じていません')


def builder_copies(dockerfile):
    active = found = False
    workdir = PurePosixPath('/')
    copies = []
    for name, body in instructions(dockerfile):
        if name == 'FROM':
            words = shlex.split(body)
            stage = words[-1].lower() if len(words) >= 3 and words[-2].upper() == 'AS' else None
            active = stage == 'builder'
            if active:
                if found or any(w.startswith('--') for w in words) or len(words) != 3:
                    raise ContextError('重複または未対応の builder FROM 指示')
                # 前ステージを継承すると COPY 以外の入力が入るため拒否。
                if ':' not in words[0] and '@' not in words[0]:
                    raise ContextError('builder のベースは明示的なイメージのタグが必要です')
                found = True
                workdir = PurePosixPath('/')
        elif active and name == 'WORKDIR':
            words = shlex.split(body)
            if len(words) != 1:
                raise ContextError(f'未対応の WORKDIR: {body}')
            workdir = image_path(words[0], workdir)
        elif active and name == 'COPY':
            if body.startswith('--'):
                # web の成果物は現在 runtime のみで使用。builder に追加されたら
                # 無条件に省略せず、この検査でも生成するか除外理由を個別に定義する。
                raise ContextError(f'未対応の COPY オプション（--from は入力を再現しません）: {body}')
            words = json.loads(body) if body.startswith('[') else shlex.split(body)
            if not isinstance(words, list) or len(words) < 2 or not all(isinstance(w, str) for w in words):
                raise ContextError(f'不正な COPY: {body}')
            for src in words[:-1]:
                literal(src)
            copies.append((words[:-1], image_path(words[-1], workdir), words[-1].endswith('/')))
        elif active and name in ('ADD', 'ONBUILD', 'VOLUME'):
            raise ContextError(f'builder の {name} は入力再現に未対応です')
    if not found or not copies:
        raise ContextError('COPY を持つ builder ステージがありません')
    return copies, workdir


def glob_regex(pattern):
    # Docker のパターンはルート基準。**/ は 0 個以上のディレクトリ。
    result = ''
    i = 0
    while i < len(pattern):
        if pattern[i:i + 3] == '**/':
            result += '(?:.*/)?'
            i += 3
        elif pattern[i:i + 2] == '**':
            result += '.*'
            i += 2
        elif pattern[i] == '*':
            result += '[^/]*'
            i += 1
        elif pattern[i] == '?':
            result += '[^/]'
            i += 1
        else:
            result += re.escape(pattern[i])
            i += 1
    return re.compile('^' + result + '$')


class Ignore:
    def __init__(self, path):
        self.rules = []
        if path.exists():
            for raw in path.read_text().splitlines():
                if raw.startswith('#') or not raw.strip():
                    continue
                line = raw.strip()
                negate = line.startswith('!')
                line = line[1:] if negate else line
                if not line or any(c in line for c in '[]\\') or '..' in PurePosixPath(line).parts:
                    raise ContextError(f'未対応の dockerignore パターン: {raw}')
                line = str(PurePosixPath(line)).strip('/')
                if line != '.':
                    self.rules.append((glob_regex(line), negate))

    def excluded(self, relative):
        parts = PurePosixPath(relative).parts
        ancestors = ['/'.join(parts[:i]) for i in range(1, len(parts) + 1)]
        excluded = False
        for pattern, negate in self.rules:
            if any(pattern.fullmatch(p) for p in ancestors):
                excluded = not negate
        return excluded


def materialize(repo, root):
    dockerfile = repo / 'Dockerfile'
    # Dockerfile 固有の ignore がある場合はそちらを優先（Docker と同じ）。
    specific = repo / 'Dockerfile.dockerignore'
    ignore = Ignore(specific if specific.exists() else repo / '.dockerignore')
    copies, workdir = builder_copies(dockerfile)
    for sources, destination, trailing in copies:
        dest = root / str(destination).lstrip('/')
        if len(sources) > 1 and not trailing:
            raise ContextError('複数ソースの COPY の宛先は / で終わる必要があります')
        for source in sources:
            relative = source.lstrip('/')
            if '..' in PurePosixPath(relative).parts:
                raise ContextError(f'コンテキスト外への参照: {source}')
            src = repo / relative
            if src.is_symlink():
                raise ContextError(f'シンボリックリンクの COPY は未対応: {source}')
            if not src.exists() or (not src.is_dir() and ignore.excluded(relative)):
                raise ContextError(f'COPY 元が存在しないか dockerignore で除外されています: {source}')
            entries = sorted(src.rglob('*')) if src.is_dir() else [src]
            if src.is_dir():
                if ignore.excluded(relative) and not any(
                    not ignore.excluded(p.relative_to(repo).as_posix()) for p in entries
                ):
                    raise ContextError(f'COPY 元が dockerignore で除外されています: {source}')
                dest.mkdir(parents=True, exist_ok=True)
            for entry in entries:
                if ignore.excluded(entry.relative_to(repo).as_posix()):
                    continue
                if entry.is_symlink():
                    raise ContextError(f'シンボリックリンクの COPY は未対応: {entry.relative_to(repo)}')
                target = dest / entry.relative_to(src) if src.is_dir() else dest / src.name if trailing or dest.is_dir() else dest
                if entry.is_dir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(entry, target)
    return root / str(workdir).lstrip('/')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parent.parent)
    args = parser.parse_args()
    started = time.monotonic()
    repo = args.repo.resolve()
    env = os.environ.copy()
    env['CARGO_TARGET_DIR'] = str(Path(env.get('CARGO_TARGET_DIR', repo / 'target')).resolve())
    try:
        with tempfile.TemporaryDirectory(prefix='docker-builder-') as temp:
            workdir = materialize(repo, Path(temp))
            result = subprocess.run(['cargo', 'check', '--release', '--locked'], cwd=workdir, env=env)
            if result.returncode:
                print('Docker の builder ステージにコピーされていないファイルを Rust が読んでいます。'
                      '上の cargo エラーに該当する場合は COPY と dockerignore を確認してください。'
                      'その他のコンパイルエラーも上の出力を確認してください。', file=sys.stderr, flush=True)
            return result.returncode
    except (ContextError, ValueError) as error:
        print(f'Docker builder の入力再現に失敗: {error}', file=sys.stderr)
        return 1
    finally:
        print(f'Docker builder 検査所要時間: {time.monotonic() - started:.1f} 秒', flush=True)


if __name__ == '__main__':
    sys.exit(main())
