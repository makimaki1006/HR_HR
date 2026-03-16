"""
V2ハローワーク分析 共通モジュール
=================================
全compute_v2_*.pyスクリプトで共有する関数・定数を定義。
"""


def emp_group(et: str) -> str:
    """雇用形態を3グループに正規化。

    部分一致で判定し、"キャリア採用/正社員" や "嘱託採用/正社員" 等の
    複合型表記も正しく「正社員」に分類する。
    """
    if not et or not isinstance(et, str):
        return "その他"
    if "正社員" in et:
        return "正社員"
    if "パート" in et:
        return "パート"
    return "その他"
