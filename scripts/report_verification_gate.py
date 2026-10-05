"""Check report records; arithmetic and evidence integrity, not semantic truth."""
import argparse
from decimal import Decimal, InvalidOperation
import hashlib
import json
from pathlib import Path


def validate(record, base):
    errors, calculations = [], []

    def evidence(items, label):
        if not isinstance(items, list) or not items:
            errors.append(f'{label}: evidence required')
            return
        for item in items:
            try:
                path = Path(item['path'])
                path = path if path.is_absolute() else base / path
                expected = item['sha256']
                if not isinstance(expected, str) or len(expected) != 64:
                    raise ValueError()
                if hashlib.sha256(path.read_bytes()).hexdigest() != expected.lower():
                    raise ValueError()
            except (KeyError, TypeError, ValueError, OSError):
                errors.append(f'{label}: missing or changed evidence')

    claims = record.get('claims') if isinstance(record, dict) else None
    if not isinstance(claims, list) or not claims:
        return {'ok': False, 'errors': ['claims required'], 'calculations': []}
    seen = set()
    for index, claim in enumerate(claims):
        label = f'claim[{index}]'
        if not isinstance(claim, dict):
            errors.append(f'{label}: object required')
            continue
        for key in ('id', 'assertion', 'scope'):
            if not isinstance(claim.get(key), str) or not claim[key].strip():
                errors.append(f'{label}: {key} required')
        identity = claim.get('id')
        if isinstance(identity, str):
            if identity in seen:
                errors.append(f'{label}: duplicate id')
            seen.add(identity)
        category = claim.get('classification')
        if category not in ('verified', 'conditional', 'unknown'):
            errors.append(f'{label}: invalid classification')
        limits = claim.get('limitations')
        if not isinstance(limits, list) or any(not isinstance(x, str) or not x.strip() for x in limits):
            errors.append(f'{label}: limitations list required')
        elif category in ('conditional', 'unknown') and not limits:
            errors.append(f'{label}: unverified conditions required')
        evidence(claim.get('evidence'), label)
        cases = claim.get('counterexamples')
        if not isinstance(cases, list) or not cases:
            errors.append(f'{label}: counterexamples required')
            continue
        for case in cases:
            if not isinstance(case, dict) or not isinstance(case.get('scenario'), str) or not case['scenario'].strip():
                errors.append(f'{label}: counterexample scenario required')
                continue
            status = case.get('disposition')
            if status not in ('verified_handled', 'design_only', 'unresolved'):
                errors.append(f'{label}: refuted or invalid counterexample')
            elif category == 'verified' and status != 'verified_handled':
                errors.append(f'{label}: verified claim has unverified counterexample')
            if status == 'verified_handled':
                evidence(case.get('evidence'), label + '/counterexample')

    def number(value):
        if isinstance(value, (bool, float)) or value is None:
            raise ValueError()
        result = Decimal(str(value))
        if not result.is_finite():
            raise ValueError()
        return result

    checks = record.get('calculations', [])
    if not isinstance(checks, list):
        errors.append('calculations must be a list')
        checks = []
    for index, check in enumerate(checks):
        label = f'calculation[{index}]'
        try:
            if not isinstance(check, dict) or not isinstance(check.get('unit'), str) or not check['unit'].strip():
                raise ValueError()
            if not isinstance(check.get('inputs'), list) or not check['inputs']:
                raise ValueError()
            values = [number(x) for x in check['inputs']]
            op = check['operation']
            if op == 'sum':
                result = sum(values, Decimal(0))
            elif op == 'product':
                result = Decimal(1)
                for value in values:
                    result *= value
            elif op in ('difference', 'ratio') and len(values) == 2:
                result = values[0] - values[1] if op == 'difference' else values[0] / values[1]
            else:
                raise ValueError()
            tolerance = number(check.get('tolerance', '0'))
            if tolerance < 0 or abs(result - number(check['expected'])) > tolerance:
                raise ValueError()
            calculations.append({'index': index, 'value': str(result), 'unit': check['unit']})
        except (KeyError, TypeError, ValueError, InvalidOperation, ArithmeticError):
            errors.append(f'{label}: invalid or mismatched calculation')
    return {'ok': not errors, 'errors': errors, 'calculations': calculations}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('record', type=Path)
    args = parser.parse_args()
    try:
        result = validate(json.loads(args.record.read_text(encoding='utf-8')), Path(__file__).resolve().parents[1])
    except (OSError, ValueError):
        result = {'ok': False, 'errors': ['record unreadable'], 'calculations': []}
    print(json.dumps(result, ensure_ascii=False))
    raise SystemExit(0 if result['ok'] else 1)
