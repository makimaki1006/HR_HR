"""第 2 段の確かめ: 型ごとに Claude Sonnet 5.5 のお客さん役と短い通しの会話を 1 回流し、全文を out/ に書く。
前提: cargo test --lib call_practice::roleplay::tests::dump_prompts -- --ignored で out/prompts.json を作る (API は呼ばない)。
使い方: python proto_stage2.py   鍵: ANTHROPIC_API_KEY_FILE (ファイルの中身。値は表示しない)
営業側は決まった台詞。日時の打診は 1〜4 回目まで順に出し、受けた型はそのあと 4 項目の質問を 2 つ続ける。"""
import json, os, re, time
from pathlib import Path
import anthropic

HERE = Path(__file__).parent
P = json.loads((HERE / "out/prompts.json").read_text())
c = anthropic.Anthropic(api_key=Path(os.environ["ANTHROPIC_API_KEY_FILE"]).read_text().strip())
OFFERS = ["来週の水曜か木曜あたりはいかがですか", "では再来週の火曜か水曜はいかがですか", "来月の3日の午前はいかがですか", "では10日の午後はいかがですか"]
HEAR = ["直近3か月以内で、採用が必要な職種はありますか？", "今の採用で困っていることと、どのくらい早く解決したいかを教えていただけますか？"]


def call(system, msgs):
    t0 = time.time()
    r = c.messages.create(model="claude-sonnet-5-5", max_tokens=400, system=[{"type": "text", "text": system, "cache_control": {"type": "ephemeral"}}],
                          messages=msgs, thinking={"type": "between_tools"}, output_config={"effort": "low"})
    text = "".join(b.text for b in r.content if b.type == "text")
    body = text.split("###")[0].strip()
    meta = {}
    if "###" in text:
        m = re.search(r"\{.*\}", text.split("###", 1)[1], re.S)
        try: meta = json.loads(m.group(0)) if m else {}
        except Exception: pass
    return body, meta, time.time() - t0


def main():
    out = []
    for pid, d in P.items():
        sysm, hist = d["system_full"], []
        lines = [("customer", d["opening"])]
        script = [("intro", "採用のご支援の件でお電話しました")] + [("offer", o) for o in OFFERS]
        offers, agreed, hq, secs = 0, False, 0, []
        i = 0
        while True:
            if not agreed:
                if i >= len(script): break
                kind, text = script[i]; i += 1
                if kind == "offer":
                    offers += 1; memo = d["memos"][offers]
                else:
                    memo = d["memos"][0]
            else:
                if hq >= len(HEAR): break
                text = HEAR[hq]; memo = f"\n\n(進行メモ: 営業の質問はここまで{hq}問。月額定額はまだ伝えられていない。嫌がる段階: なし。)"; hq += 1
            body, meta, sec = call(sysm, hist + [{"role": "user", "content": text + memo}])
            secs.append(sec)
            hist += [{"role": "user", "content": text}, {"role": "assistant", "content": body}]
            lines += [("rep", text), ("customer", body)]
            if not agreed and offers and d["accept_after"] and offers >= d["accept_after"] and script[i - 1][0] == "offer":
                agreed = True
            if meta.get("end"): break
        out.append(f"## {pid} ({d['setting']}) 受ける打診={d['accept_after']}\n" + "\n".join(f"- {'営' if s == 'rep' else '先'}: {t}" for s, t in lines) + f"\n(返事の秒数 最大 {max(secs):.1f} / 中央 {sorted(secs)[len(secs)//2]:.1f})\n")
    stamp = time.strftime("%Y%m%d-%H%M%S")
    (HERE / f"out/stage2-{stamp}.md").write_text("\n".join(out))
    print("\n".join(out))


if __name__ == "__main__":
    main()
