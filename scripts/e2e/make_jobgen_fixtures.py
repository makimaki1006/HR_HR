"""架空の顧客受領資料。標準ライブラリだけで再生成できる。"""
import csv
import io
from pathlib import Path
from xml.sax.saxutils import escape
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[2] / "tests/fixtures/jobgen"
ROOT.mkdir(parents=True, exist_ok=True)
rows = [
    ["管理番号", "職種名", "給与", "勤務時間", "休日", "勤務地", "雇用形態", "仕事内容"],
    ["SAMPLE-001", "倉庫スタッフ", "月給250,000円", "9:00〜18:00", "土日休み", "東京都江東区架空1-2-3", "正社員", '商品の検品、梱包\n商品に「傷」がないか確認'],
    ["SAMPLE-002", "配送スタッフ", "月給300,000円", "8:00〜17:00", "水日休み", "大阪府大阪市架空4-5-6", "正社員", "架空店舗への配送"],
]
buf = io.StringIO(newline="")
csv.writer(buf).writerows(rows)
text = buf.getvalue()
(ROOT / "customer-utf8.csv").write_bytes(text.encode("utf-8-sig"))
(ROOT / "customer-sjis.csv").write_bytes(text.encode("cp932"))
(ROOT / "customer-invalid.csv").write_text("職種名,給与\n倉庫スタッフ,250000,消えてはいけない値\n", encoding="utf-8")
(ROOT / "customer.txt").write_text("\n".join(f"{k}: {v}" for k, v in zip(rows[0][1:], rows[1][1:])), encoding="utf-8")

sheet = '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>'
for n, row in enumerate(rows, 1):
    sheet += f'<row r="{n}">' + "".join(f'<c r="{chr(65+i)}{n}" t="inlineStr"><is><t>{escape(v)}</t></is></c>' for i, v in enumerate(row)) + '</row>'
sheet += '</sheetData></worksheet>'
with ZipFile(ROOT / "customer.xlsx", "w") as z:
    z.writestr("[Content_Types].xml", '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>')
    z.writestr("_rels/.rels", '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>')
    z.writestr("xl/workbook.xml", '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="求人一覧" sheetId="1" r:id="rId1"/></sheets></workbook>')
    z.writestr("xl/_rels/workbook.xml.rels", '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>')
    z.writestr("xl/worksheets/sheet1.xml", sheet)


def pdf(name, image=False):
    content = b"q 200 0 0 200 72 500 cm /Im1 Do Q" if image else b"BT /F1 18 Tf 72 700 Td (Warehouse staff) Tj 0 -30 Td (Salary: 250000 yen/month) Tj 0 -30 Td (Hours: 9:00-18:00) Tj 0 -30 Td (Holidays: Saturday and Sunday) Tj 0 -30 Td (Location: Fictional Tokyo warehouse) Tj 0 -30 Td (Employment: Full-time) Tj ET"
    image_bytes = (ROOT / "customer-scan.jpg").read_bytes()
    if image:
        content = b"q 450 0 0 600 72 100 cm /Im1 Do Q"
    image_object = f'<</Type/XObject/Subtype/Image/Width 600/Height 800/ColorSpace/DeviceRGB/BitsPerComponent 8/Filter/DCTDecode/Length {len(image_bytes)}>>stream\n'.encode() + image_bytes + b'\nendstream'
    objs = [b'<</Type/Catalog/Pages 2 0 R>>', b'<</Type/Pages/Kids[3 0 R]/Count 1>>', b'<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]/Resources<</Font<</F1 4 0 R>>/XObject<</Im1 6 0 R>>>>/Contents 5 0 R>>', b'<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>', f'<</Length {len(content)}>>stream\n'.encode() + content + b'\nendstream', image_object]
    data = b"%PDF-1.4\n"
    offsets = []
    for i, obj in enumerate(objs, 1):
        offsets.append(len(data))
        data += f"{i} 0 obj\n".encode() + obj + b"\nendobj\n"
    xref = len(data)
    data += f"xref\n0 {len(objs)+1}\n0000000000 65535 f \n".encode()
    data += b"".join(f"{offset:010} 00000 n \n".encode() for offset in offsets)
    data += f"trailer<</Size {len(objs)+1}/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF".encode()
    (ROOT / name).write_bytes(data)


pdf("customer-text.pdf")
pdf("customer-image.pdf", image=True)
