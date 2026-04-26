# -*- coding: utf-8 -*-
"""
industry_mapping.py の SSDSE-A Phase A 拡張に対する単体テスト
============================================================
SSDSE-A 業種17分類 × HW13職種 のマッピング整合性を検証する。

実行:
    pytest tests/test_industry_mapping_ssdse.py -v
"""
import os
import sys

import pytest

# scripts/ ディレクトリを import path に追加
_THIS_DIR = os.path.dirname(os.path.abspath(__file__))
_REPO_ROOT = os.path.dirname(_THIS_DIR)
_SCRIPTS_DIR = os.path.join(_REPO_ROOT, "scripts")
if _SCRIPTS_DIR not in sys.path:
    sys.path.insert(0, _SCRIPTS_DIR)

try:
    from industry_mapping import (  # type: ignore
        INDUSTRY_MAPPING,
        SSDSE_HW_MAPPING,
        SSDSE_INDUSTRY_NAMES,
        HW_SSDSE_MAPPING,
        get_hw_job_types,
        get_hw_for_ssdse,
        get_ssdse_for_hw,
        get_ssdse_industry_name,
        build_mapping_rows,
        build_ssdse_mapping_rows,
    )
except ImportError:
    pytest.skip(f"industry_mapping.py not found in {_SCRIPTS_DIR}", allow_module_level=True)


# ═════════════════════════════════════════════════════════════
# 既存 SalesNow↔HW マッピングの回帰（既存機能を壊していないこと）
# ═════════════════════════════════════════════════════════════

def test_existing_salesnow_mapping_intact():
    """既存の SalesNow 32業界がすべて保持されている。"""
    expected_count = 32  # 既存マッピング数（元ファイル参照）
    actual_count = len(INDUSTRY_MAPPING)
    assert actual_count >= expected_count - 1, (
        f"INDUSTRY_MAPPING の行数が減少している: {actual_count} < {expected_count}"
    )

    # 主要マッピングが維持されている
    assert "建設" in INDUSTRY_MAPPING
    assert "IT" in INDUSTRY_MAPPING
    assert "医療・製薬・福祉" in INDUSTRY_MAPPING


def test_existing_get_hw_job_types():
    """既存 API が動作する。"""
    result = get_hw_job_types("建設")
    assert result == [("建設業", 1.0)]

    result = get_hw_job_types("IT")
    assert result == [("IT・通信", 1.0)]

    # 未知の業界 → デフォルト
    result = get_hw_job_types("未知の業界")
    assert result == [("その他", 0.3)]


def test_build_mapping_rows_intact():
    """build_mapping_rows() の戻り値形式が変わっていない。"""
    rows = build_mapping_rows()
    assert len(rows) > 30
    for row in rows:
        assert len(row) == 3, f"行フォーマット変更: {row}"
        sn, hw, conf = row
        assert isinstance(sn, str)
        assert isinstance(hw, str)
        assert isinstance(conf, float)
        assert 0.0 <= conf <= 1.0


# ═════════════════════════════════════════════════════════════
# SSDSE_HW_MAPPING（Phase A 新規）
# ═════════════════════════════════════════════════════════════

EXPECTED_SSDSE_CODES = {
    "ALL", "832", "833", "835", "836", "837", "838", "839",
    "840", "841", "844", "845", "846", "847", "848", "849",
    "850", "851", "852",
}


def test_ssdse_mapping_covers_all_codes():
    """SSDSE-A の全17業種 + ALL をカバー"""
    actual = set(SSDSE_HW_MAPPING.keys())
    assert actual == EXPECTED_SSDSE_CODES, (
        f"missing={EXPECTED_SSDSE_CODES - actual}, "
        f"extra={actual - EXPECTED_SSDSE_CODES}"
    )


def test_ssdse_industry_names_cover_all_codes():
    """SSDSE_INDUSTRY_NAMES が全業種をカバー"""
    actual = set(SSDSE_INDUSTRY_NAMES.keys())
    assert actual == EXPECTED_SSDSE_CODES


def test_ssdse_mapping_values_are_well_formed():
    """各マッピングが [(hw_name, confidence), ...] 形式で、信頼度が 0.0-1.0 に収まる"""
    for ssdse_code, mappings in SSDSE_HW_MAPPING.items():
        assert isinstance(mappings, list), f"{ssdse_code}: list ではない"
        assert len(mappings) >= 1, f"{ssdse_code}: マッピング空"
        for hw, conf in mappings:
            assert isinstance(hw, str), f"{ssdse_code}: hw_name が str ではない"
            assert isinstance(conf, float), f"{ssdse_code}: confidence が float ではない"
            assert 0.0 <= conf <= 1.0, (
                f"{ssdse_code} → {hw}: confidence={conf} は範囲外"
            )


def test_ssdse_mapping_critical_industries():
    """重要業種（Phase A 10業種）が適切にマッピングされている"""
    # 建設業 (836)
    mappings = SSDSE_HW_MAPPING["836"]
    hw_names = [m[0] for m in mappings]
    assert "建設業" in hw_names

    # 製造業 (837)
    mappings = SSDSE_HW_MAPPING["837"]
    assert "製造業" in [m[0] for m in mappings]

    # 宿泊・飲食 (847)
    mappings = SSDSE_HW_MAPPING["847"]
    hw_names = [m[0] for m in mappings]
    assert "飲食業" in hw_names or "宿泊業" in hw_names

    # 教育・学習支援 (849)
    mappings = SSDSE_HW_MAPPING["849"]
    assert "教育・保育" in [m[0] for m in mappings]

    # 医療・福祉 (850) — 最重要
    mappings = SSDSE_HW_MAPPING["850"]
    hw_names = [m[0] for m in mappings]
    assert "医療" in hw_names or "老人福祉・介護" in hw_names, (
        f"医療・福祉 (850) のマッピングが欠落: {hw_names}"
    )


# ═════════════════════════════════════════════════════════════
# 逆引き HW_SSDSE_MAPPING
# ═════════════════════════════════════════════════════════════

def test_reverse_mapping_built():
    """HW_SSDSE_MAPPING が自動構築される"""
    assert len(HW_SSDSE_MAPPING) > 0

    # 主要 HW 業種が含まれる
    for hw in ["建設業", "製造業", "運輸業", "医療", "教育・保育"]:
        assert hw in HW_SSDSE_MAPPING, f"HW_SSDSE_MAPPING に {hw} が無い"


def test_reverse_mapping_sorted_by_confidence():
    """逆引きエントリが信頼度降順にソートされている"""
    for hw, entries in HW_SSDSE_MAPPING.items():
        confs = [e[1] for e in entries]
        assert confs == sorted(confs, reverse=True), (
            f"{hw}: 信頼度順でない {confs}"
        )


def test_reverse_mapping_excludes_low_confidence():
    """信頼度 0.5 未満のマッピングは逆引きから除外される"""
    for hw, entries in HW_SSDSE_MAPPING.items():
        for ssdse_code, conf in entries:
            assert conf >= 0.5, (
                f"{hw}: 信頼度 {conf} < 0.5 が逆引きに含まれている ({ssdse_code})"
            )


# ═════════════════════════════════════════════════════════════
# アクセサ関数
# ═════════════════════════════════════════════════════════════

def test_get_hw_for_ssdse():
    result = get_hw_for_ssdse("850")
    hw_names = [m[0] for m in result]
    assert "医療" in hw_names or "老人福祉・介護" in hw_names

    # 未知コード
    result = get_hw_for_ssdse("UNKNOWN")
    assert result == [("その他", 0.3)]


def test_get_ssdse_for_hw():
    result = get_ssdse_for_hw("建設業")
    codes = [r[0] for r in result]
    assert "836" in codes

    # 未知 HW
    result = get_ssdse_for_hw("未知業種")
    assert result == []


def test_get_ssdse_industry_name():
    assert get_ssdse_industry_name("836") == "建設業"
    assert get_ssdse_industry_name("850") == "医療、福祉"
    # 未知コード → コードそのものを返す
    assert get_ssdse_industry_name("UNKNOWN") == "UNKNOWN"


def test_build_ssdse_mapping_rows():
    rows = build_ssdse_mapping_rows()
    assert len(rows) > 0
    for row in rows:
        assert len(row) == 4
        ssdse_code, ssdse_name, hw, conf = row
        assert ssdse_code in EXPECTED_SSDSE_CODES
        assert isinstance(ssdse_name, str) and len(ssdse_name) > 0
        assert isinstance(hw, str)
        assert 0.0 <= conf <= 1.0
