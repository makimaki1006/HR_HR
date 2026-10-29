"""架電の練習アプリ 第 0 段の試作: お客さん役を MiniMax に演じさせ、速さ・言語の混ざり・設定の漏れ・型外れを数える。

使い方: python proto.py [回数] [モデル...]   (既定: 2 回、MiniMax-M3 と MiniMax-M2.7-highspeed)
鍵は環境変数 MINIMAX_API_KEY、無ければ ENV_FILE (既定 ~/Downloads/env) から読む。値は表示しない。
結果は標準出力の表と、out/ の JSONL (会話の全文)。
"""
import json, os, re, sys, time
from pathlib import Path
import requests

HERE = Path(__file__).parent
URL = os.environ.get("MINIMAX_BASE_URL", "https://api.minimax.io/v1").rstrip("/") + "/chat/completions"
# src/crm/talk_review/lang.rs と同じ考え方 (簡体字にしかない字・3 文字以上の英字・ハングル)
SIMPLIFIED_ONLY = "这个们为说对时过还吗让给开发见现观关从么样机间问题应该别实种经办总须户买无协话语请认识谢订记讨论设计达进运动车软单业务员产价优势复习质评钱银长门阳际标线级约热术议导众战听觉读"


def key():
    k = os.environ.get("MINIMAX_API_KEY")
    if k:
        return k.strip()
    env = Path(os.environ.get("ENV_FILE", Path.home() / "Downloads/env"))
    for line in env.read_text().replace("\r", "").splitlines():
        if line.startswith("MINIMAX_API_KEY="):
            return line.split("=", 1)[1].strip().strip('"')
    sys.exit("MINIMAX_API_KEY がありません")


def foreign(text):
    for c in text:
        if c in SIMPLIFIED_ONLY or 0xAC00 <= ord(c) <= 0xD7FF or c in "，":
            return c
    m = re.search(r"[A-Za-z]{3,}", text)
    return m.group(0) if m else None


def strip_think(text):
    text = re.sub(r"<think>.*?</think>", "", text, flags=re.S)
    return re.sub(r"</?(mm:)?think>", "", text).strip()


def stream(k, model, msgs):
    """返事を流しながら受け取り、(最初の文字までの秒, 全文の秒, 本文) を返す"""
    t0, first, out = time.time(), None, ""
    body = {"model": model, "messages": msgs, "stream": True, "max_tokens": 1500,
            "temperature": float(os.environ.get("CALL_PRACTICE_TEMPERATURE", "0.7")), "reasoning_split": True}
    with requests.post(URL, headers={"Authorization": "Bearer " + k}, json=body, stream=True, timeout=30) as r:
        if r.status_code != 200:
            raise RuntimeError(f"HTTP {r.status_code}: {r.text[:200]}")
        for line in r.iter_lines():
            if not line.startswith(b"data:"):
                continue
            d = line[5:].strip()
            if d == b"[DONE]":
                break
            j = json.loads(d)
            err = (j.get("base_resp") or {}).get("status_code")
            if err:
                raise RuntimeError(f"MiniMax {err}: {j['base_resp'].get('status_msg')}")
            c = ((j.get("choices") or [{}])[0].get("delta") or {}).get("content") or ""
            if first is None and c.strip():
                first = time.time() - t0
            out += c
    return first, time.time() - t0, strip_think(out)


def haiku(msgs, model="claude-haiku-5-5"):
    """崩れたときの代わり: Claude Haiku に同じ会話で作らせる (鍵は ANTHROPIC_API_KEY_FILE)"""
    import anthropic
    c = anthropic.Anthropic(api_key=Path(os.environ["ANTHROPIC_API_KEY_FILE"]).read_text().strip())
    t0, first, out = time.time(), None, ""
    extra = {"thinking": {"type": "between_tools"}} if "sonnet" in model else {}
    with c.messages.stream(model=model, max_tokens=300, system=msgs[0]["content"],
                           messages=msgs[1:], output_config={"effort": "low"}, **extra) as s:
        for ev in s.text_stream:
            if first is None and ev.strip():
                first = time.time() - t0
            out += ev
    return first, time.time() - t0, out.strip()


def reply(k, model, msgs):
    """混ざっていたら CALL_PRACTICE_RETRIES 回 (既定 0) まで作り直し、それでも混ざれば
    ANTHROPIC_API_KEY_FILE があるとき Haiku に作らせる。画面に出すのは検査を通った文"""
    retries = int(os.environ.get("CALL_PRACTICE_RETRIES", "0"))
    if model.startswith("claude-"):
        f, tot, text = haiku(msgs, model)
        return {"first": f, "total": tot, "text": text, "mixed_first": foreign(text), "retries": 0,
                "by": model, "mixed_final": foreign(text)}
    f, tot, text = stream(k, model, msgs)
    bad1 = foreign(text)
    n, by = 0, model
    while foreign(text) and n < retries:
        n += 1
        _, t2, text = stream(k, model, msgs)
        tot += t2
    if foreign(text) and os.environ.get("ANTHROPIC_API_KEY_FILE"):
        _, t2, text = haiku(msgs)
        tot += t2
        by = "claude-haiku-5-5"
    return {"first": f, "total": tot, "text": text, "mixed_first": bad1, "retries": n,
            "by": by, "mixed_final": foreign(text)}


def leaks(persona, asked, text):
    """まだ聞かれていない項目の言葉が返事に出ていれば、その項目を返す"""
    return [item for item, words in persona["facts"].items()
            if item not in asked and any(w in text for w in words)]


def run(k, model, tname, persona, sname, script, common):
    system = common + "\n\n人物設定: " + persona["setting"]
    msgs, asked, rows = [{"role": "system", "content": system}], set(), []
    for turn in script:
        asked.add(turn["item"].rstrip("2"))
        msgs.append({"role": "user", "content": turn["text"]})
        r = reply(k, model, msgs)
        r["leak"] = leaks(persona, asked, r["text"])
        r["off"] = [w for w in persona["contradict"] if w in r["text"]]
        r["sentences"] = len([s for s in re.split(r"[。！？!?]", r["text"]) if s.strip()])
        r.update(model=model, type=tname, script=sname, ask=turn["text"])
        rows.append(r)
        msgs.append({"role": "assistant", "content": r["text"] if not r["mixed_final"] else "はい。"})
    return rows


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 2
    models = sys.argv[2:] or ["MiniMax-M3", "MiniMax-M2.7-highspeed"]
    k = key()
    p = json.loads((HERE / "personas.json").read_text())
    s = json.loads((HERE / "scripts.json").read_text())
    out_dir = HERE / "out"
    out_dir.mkdir(exist_ok=True)
    rows = []
    for model in models:
        for i in range(n):
            for tname, persona in p["types"].items():
                for sname in ("good", "bad"):
                    rows += run(k, model, tname, persona, sname, s[sname], p["common_rules"])
    stamp = time.strftime("%Y%m%d-%H%M%S")
    with open(out_dir / f"run-{stamp}.jsonl", "w") as f:
        for r in rows:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print("モデル | 返事 | 最初の文字(中央/最大) | 全文(中央/最大) | 混ざり(最初) | 混ざり(作り直し後) | 漏れ | 型外れ | 3文以上")
    for model in models:
        rs = [r for r in rows if r["model"] == model]
        fs = sorted(r["first"] or r["total"] for r in rs)
        ts = sorted(r["total"] for r in rs)
        med = lambda a: a[len(a) // 2]
        print(f"{model} | {len(rs)} | {med(fs):.2f}/{fs[-1]:.2f} | {med(ts):.2f}/{ts[-1]:.2f} | "
              f"{sum(bool(r['mixed_first']) for r in rs)} | {sum(bool(r['mixed_final']) for r in rs)} | "
              f"{sum(bool(r['leak']) for r in rs)} | {sum(bool(r['off']) for r in rs)} | "
              f"{sum(r['sentences'] >= 3 for r in rs)}")
    print("出力:", out_dir / f"run-{stamp}.jsonl")


if __name__ == "__main__":
    main()
