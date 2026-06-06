#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
pipeline.py — データソース統合パイプライン CLI

data_sources.yaml (レジストリ) を読み、「○○のデータ更新」を共通フローで回す。
都度ロジックを書かず、レジストリ定義に従って 取得→加工→(投入指示)→反映 を実行する。

[MANUAL] CLAUDE.md 厳守: Claude による DB 書き込み禁止。
   本スクリプトの upload(投入) フェーズは「手動実行コマンドを表示」するのみで、
   自動実行しない。実際の Turso 投入はユーザーが手動で行う。

使い方:
    python pipeline.py --list                    # 全データソース一覧
    python pipeline.py --list --frequency yearly # 頻度で絞り込み
    python pipeline.py --list --status hidden    # 状態で絞り込み (死蔵データ抽出)
    python pipeline.py --show minimum_wage_history   # 1件の詳細手順
    python pipeline.py --update household_spending --dry-run  # 実行コマンド表示のみ
    python pipeline.py --update household_spending    # fetch→transform 実行(uploadは手動指示)
    python pipeline.py --validate                # レジストリ整合性チェック
    python pipeline.py --runbook yearly          # 頻度別ランブック (一括更新手順)
"""

import argparse
import subprocess
import sys
from pathlib import Path

# Windows cp932 でも絵文字混在でクラッシュしないよう stdout/stderr を UTF-8 化
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except Exception:
        pass

try:
    import yaml
except ImportError:
    print("PyYAML が必要です: pip install pyyaml", file=sys.stderr)
    sys.exit(1)

SCRIPTS_DIR = Path(__file__).resolve().parent
REGISTRY_PATH = SCRIPTS_DIR / "data_sources.yaml"

# 表示中の4タブ (これ以外=非表示タブ。新規データは必ず表示4タブに接続すべき)
VISIBLE_TABS = {"jobmap", "survey", "competitive", "company"}
KNOWN_FREQUENCIES = {
    "monthly", "quarterly", "yearly", "quinquennial",
    "irregular", "continuous",
}
KNOWN_STATUS = {"visible", "report", "hidden", "unused"}


def load_registry():
    """data_sources.yaml を読み込み、sources リストを返す。"""
    if not REGISTRY_PATH.exists():
        print(f"レジストリが見つかりません: {REGISTRY_PATH}", file=sys.stderr)
        sys.exit(1)
    with open(REGISTRY_PATH, encoding="utf-8") as f:
        data = yaml.safe_load(f)
    sources = data.get("sources", [])
    if not sources:
        print("レジストリに sources が空です", file=sys.stderr)
        sys.exit(1)
    return sources


def find(sources, name):
    for s in sources:
        if s.get("name") == name:
            return s
    return None


def cmd_list(sources, frequency=None, status=None):
    """データソース一覧。frequency / status で絞り込み可。"""
    rows = sources
    if frequency:
        rows = [s for s in rows if s.get("frequency") == frequency]
    if status:
        rows = [s for s in rows if s.get("status") == status]
    if not rows:
        print("該当なし")
        return
    print(f"{'name':28} {'source':14} {'freq':13} {'status':8} tabs")
    print("-" * 90)
    for s in rows:
        tabs = ",".join(s.get("tabs") or []) or "(なし)"
        print(
            f"{s['name']:28} {s.get('source',''):14} "
            f"{s.get('frequency',''):13} {s.get('status',''):8} {tabs}"
        )
    print(f"\n計 {len(rows)} 件")


def cmd_show(sources, name):
    """1データソースの取得→加工→投入→反映 手順を表示。"""
    s = find(sources, name)
    if not s:
        print(f"未登録: {name}", file=sys.stderr)
        sys.exit(1)
    print(f"=== {s['name']} ===")
    print(f"  取得元      : {s.get('source')}", end="")
    if s.get("stats_data_id"):
        print(f" (statsDataId={s['stats_data_id']})")
    else:
        print()
    print(f"  テーブル    : {s.get('table')}")
    print(f"  粒度        : {s.get('granularity')}")
    print(f"  鮮度カラム  : {s.get('freshness_col') or '[WARN] なし (要改善)'}")
    print(f"  更新頻度    : {s.get('frequency')}")
    print(f"  活用タブ    : {','.join(s.get('tabs') or []) or '(なし)'}  [{s.get('status')}]")
    if s.get("manual_consts"):
        print(f"  手動更新定数: {', '.join(s['manual_consts'])}")
    if s.get("notes"):
        print(f"  備考        : {s['notes']}")
    print("\n  --- 更新手順 (取得→加工→投入→反映) ---")
    step = 1
    if s.get("manual_consts"):
        print(f"  [{step}] (年度更新時) 手動定数を更新: {', '.join(s['manual_consts'])}")
        step += 1
    if s.get("fetch"):
        print(f"  [{step}] 取得  : python {s['fetch']}")
        step += 1
    if s.get("transform"):
        print(f"  [{step}] 加工  : python {s['transform']}")
        step += 1
    if s.get("upload"):
        print(f"  [{step}] 投入  : python {s['upload']}   ← [MANUAL] ユーザー手動実行 (Claude禁止)")
        step += 1
    print(f"  [{step}] 反映  : アプリは DB を都度参照。コード変更不要 "
          "(新規テーブル時のみ handler/route 追加)")


def _run(script_name, dry_run):
    """scripts/ 配下のスクリプトを実行 (dry_run なら表示のみ)。"""
    path = SCRIPTS_DIR / script_name
    cmd = [sys.executable, str(path)]
    if dry_run:
        print(f"    [dry-run] {' '.join(cmd)}")
        return True
    if not path.exists():
        print(f"    x スクリプト不在: {script_name}", file=sys.stderr)
        return False
    print(f"    $ {' '.join(cmd)}")
    res = subprocess.run(cmd, cwd=str(SCRIPTS_DIR))
    return res.returncode == 0


def cmd_update(sources, name, dry_run, _seen=None, _stack=None):
    """fetch→transform を実行。upload は手動指示を表示 (DB書込禁止)。

    depends_on があれば依存先を先に処理 (再帰)。循環は検出してエラー。
    """
    if _seen is None:
        _seen = set()
    if _stack is None:
        _stack = []
    s = find(sources, name)
    if not s:
        print(f"未登録: {name}", file=sys.stderr)
        sys.exit(1)
    # 循環検出
    if name in _stack:
        print(f"x 循環依存検出: {' -> '.join(_stack)} -> {name}", file=sys.stderr)
        sys.exit(1)
    if name in _seen:
        return  # 既に処理済み
    # 依存先を先に (取得→加工のみ。upload は各々の手動指示で出る)
    deps = s.get("depends_on") or []
    if deps:
        print(f"  [依存] {name} は {deps} に依存 → 先に処理")
        for dep in deps:
            cmd_update(sources, dep, dry_run, _seen, _stack + [name])
    _seen.add(name)
    print(f"=== update: {s['name']} ({'DRY-RUN' if dry_run else '実行'}) ===")

    if s.get("manual_consts"):
        print(f"  [WARN] 先に手動定数の更新を確認: {', '.join(s['manual_consts'])}")

    ok = True
    if s.get("fetch"):
        print("  [取得]")
        ok = _run(s["fetch"], dry_run) and ok
    else:
        print("  [取得] スクリプトなし (別パイプライン or 手動DL)")

    if ok and s.get("transform"):
        print("  [加工]")
        ok = _run(s["transform"], dry_run) and ok

    print("  [投入] [MANUAL] Claude は実行しません。以下をユーザーが手動実行してください:")
    if s.get("upload"):
        print(f"          python {s['upload']}")
        print("          (投入前に: python validate_all_csvs.py で検証推奨)")
    else:
        print("          (投入スクリプトなし / ローカルDB直投 or 投入不要)")

    print("  [反映] アプリは DB を都度参照。コード変更不要")
    if not ok and not dry_run:
        print("\n  x 取得/加工でエラー。ログを確認してください", file=sys.stderr)
        sys.exit(1)


def cmd_runbook(sources, frequency):
    """頻度別の一括更新ランブックを表示。"""
    rows = [s for s in sources if s.get("frequency") == frequency]
    if not rows:
        print(f"頻度 '{frequency}' のデータソースなし")
        return
    print(f"=== {frequency} 更新ランブック ({len(rows)}件) ===\n")
    # 手動定数更新がある場合は先頭に集約
    consts = sorted({c for s in rows for c in (s.get("manual_consts") or [])})
    if consts:
        print("[STEP 0] 手動定数の更新 (compute_v2_external.py / upload_*.py 内):")
        for c in consts:
            print(f"         - {c}")
        print()
    print("[STEP 1] 取得・加工 (Claude/自動で実行可):")
    for s in rows:
        if s.get("fetch") or s.get("transform"):
            print(f"         python pipeline.py --update {s['name']}")
    print("\n[STEP 2] 投入 ([MANUAL] ユーザー手動。Turso へ):")
    uploads = sorted({s["upload"] for s in rows if s.get("upload")})
    for u in uploads:
        print(f"         python {u}")
    print("\n[STEP 3] 反映: 自動 (アプリが DB を都度参照)")


def cmd_freshness(sources):
    """各データソースの鮮度 (DBカラム or レジストリ時点) を一覧。"""
    print(f"{'name':28} {'freq':13} {'鮮度カラム/時点':24} status")
    print("-" * 80)
    for s in sorted(sources, key=lambda x: (x.get("frequency", ""), x.get("name", ""))):
        col = s.get("freshness_col")
        asof = s.get("source_as_of")
        if col:
            fresh = f"col:{col}"
        elif asof:
            fresh = f"asof:{asof}"
        else:
            fresh = "[WARN] なし"
        print(f"{s['name']:28} {s.get('frequency',''):13} {fresh:24} {s.get('status','')}")


def cmd_validate(sources):
    """レジストリ整合性チェック。"""
    errors, warnings = [], []
    seen_names, seen_tables = set(), {}

    for s in sources:
        name = s.get("name", "(no name)")
        # 必須フィールド
        for req in ("name", "source", "table", "granularity", "frequency", "status"):
            if not s.get(req):
                errors.append(f"{name}: 必須フィールド '{req}' 欠落")
        # name 重複
        if name in seen_names:
            errors.append(f"{name}: name 重複")
        seen_names.add(name)
        # table 重複 (別 name が同 table)
        tbl = s.get("table")
        if tbl:
            if tbl in seen_tables and seen_tables[tbl] != name:
                warnings.append(f"{tbl}: 複数 name が同一テーブル ({seen_tables[tbl]}, {name})")
            seen_tables[tbl] = name
        # frequency / status 既知値
        if s.get("frequency") not in KNOWN_FREQUENCIES:
            errors.append(f"{name}: 未知の frequency '{s.get('frequency')}'")
        if s.get("status") not in KNOWN_STATUS:
            errors.append(f"{name}: 未知の status '{s.get('status')}'")
        # スクリプト実在
        for field in ("fetch", "transform", "upload"):
            script = s.get(field)
            if script and not (SCRIPTS_DIR / script).exists():
                errors.append(f"{name}: {field} スクリプト不在 '{script}'")
        # status と tabs の整合
        tabs = set(s.get("tabs") or [])
        visible = bool(tabs & VISIBLE_TABS)
        st = s.get("status")
        if st == "visible" and not visible:
            errors.append(f"{name}: status=visible だが表示4タブに未接続 (tabs={tabs})")
        if st == "unused" and tabs:
            warnings.append(f"{name}: status=unused だが tabs={tabs} 指定あり")
        if st == "hidden" and visible:
            warnings.append(f"{name}: status=hidden だが表示タブを含む (status見直し?)")
        # 鮮度: freshness_col(DBカラム) も source_as_of(レジストリ時点) も無い場合のみ警告
        if not s.get("freshness_col") and not s.get("source_as_of"):
            warnings.append(f"{name}: 鮮度情報なし (freshness_col か source_as_of を付与)")

    print(f"=== validate: {len(sources)} データソース ===")
    if errors:
        print(f"\n[NG] エラー {len(errors)}件:")
        for e in errors:
            print(f"  - {e}")
    if warnings:
        print(f"\n[WARN] 警告 {len(warnings)}件:")
        for w in warnings:
            print(f"  - {w}")
    if not errors and not warnings:
        print("[OK] 問題なし")
    elif not errors:
        print("\n[OK] エラーなし (警告のみ)")
    # 集計
    by_status = {}
    for s in sources:
        by_status[s.get("status")] = by_status.get(s.get("status"), 0) + 1
    print(f"\n状態別: {by_status}")
    sys.exit(1 if errors else 0)


def main():
    p = argparse.ArgumentParser(description="データソース統合パイプライン")
    p.add_argument("--list", action="store_true", help="一覧")
    p.add_argument("--show", metavar="NAME", help="1件の詳細手順")
    p.add_argument("--update", metavar="NAME", help="取得→加工を実行 (投入は手動指示)")
    p.add_argument("--runbook", metavar="FREQ", help="頻度別ランブック")
    p.add_argument("--freshness", action="store_true", help="鮮度一覧")
    p.add_argument("--validate", action="store_true", help="整合性チェック")
    p.add_argument("--frequency", metavar="FREQ", help="--list の頻度フィルタ")
    p.add_argument("--status", metavar="STATUS", help="--list の状態フィルタ")
    p.add_argument("--dry-run", action="store_true", help="--update でコマンド表示のみ")
    args = p.parse_args()

    sources = load_registry()

    if args.validate:
        cmd_validate(sources)
    elif args.freshness:
        cmd_freshness(sources)
    elif args.list:
        cmd_list(sources, frequency=args.frequency, status=args.status)
    elif args.show:
        cmd_show(sources, args.show)
    elif args.update:
        cmd_update(sources, args.update, dry_run=args.dry_run)
    elif args.runbook:
        cmd_runbook(sources, args.runbook)
    else:
        p.print_help()


if __name__ == "__main__":
    main()
