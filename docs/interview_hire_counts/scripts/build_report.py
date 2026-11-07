import csv,collections,statistics,sys
src,out=sys.argv[1],sys.argv[2]
R=lambda n:list(csv.DictReader(open(f'{src}/{n}',encoding='utf-8-sig')))
fin=R('monthly_final.csv'); inc=R('monthly_increase.csv'); nn=R('monthly_increase_no_negative.csv')
dec=R('decrease_classified.csv'); exp=R('expiry_vs_monthend.csv'); opt=R('option_vs_main.csv')
def f(s):
  try: return float(s)
  except: return 0.0
C1,C2,RED,MUT,LINE='#3b5b7a','#c08a3e','#9c3b2e','#6b6b6b','#d8d3ca'
def bars(rows,keys,lo,hi,step,h=260,lab=lambda r:r['月'][2:].replace('-','/')):
  W=760;L=46;T=10;B=h-40;n=len(rows);gw=(W-L-10)/n;bw=gw*0.38
  y=lambda v:T+(hi-v)/(hi-lo)*(B-T)
  s=[f'<svg viewBox="0 0 {W} {h}" role="img">']
  v=lo
  while v<=hi:
    s.append(f'<line x1="{L}" x2="{W-10}" y1="{y(v):.1f}" y2="{y(v):.1f}" stroke="{LINE if v else "#222"}" stroke-width="{0.6 if v else 1}"/><text x="{L-6}" y="{y(v)+4:.1f}" text-anchor="end" font-size="10" fill="{MUT}">{int(v):,}</text>');v+=step
  for i,r in enumerate(rows):
    x=L+i*gw+gw*0.1
    for j,(k,c) in enumerate(keys):
      val=f(r[k]);a,b=sorted([y(0),y(val)])
      s.append(f'<rect x="{x+j*bw:.1f}" y="{a:.1f}" width="{bw:.1f}" height="{b-a:.1f}" fill="{c}"/>')
    s.append(f'<text x="{x+bw:.1f}" y="{h-24}" text-anchor="middle" font-size="9.5" fill="{MUT}">{lab(r)}</text>')
  s.append(f'<rect x="{L}" y="{h-14}" width="10" height="10" fill="{C1}"/><text x="{L+14}" y="{h-5}" font-size="11" fill="{MUT}">面接</text><rect x="{L+60}" y="{h-14}" width="10" height="10" fill="{C2}"/><text x="{L+74}" y="{h-5}" font-size="11" fill="{MUT}">採用</text></svg>')
  return ''.join(s)
def table(head,rows,num):
  t=['<table><tr>'+''.join(f'<th class="{"n" if i in num else ""}">{h}</th>' for i,h in enumerate(head))+'</tr>']
  for r in rows: t.append('<tr>'+''.join(f'<td class="{"n" if i in num else ""}">{c}</td>' for i,c in enumerate(r))+'</tr>')
  return ''.join(t)+'</table>'
fmt=lambda v:f'{int(v):,}'
# decreases
cat=collections.Counter(x['区分'] for x in dec)
ppl=collections.defaultdict(collections.Counter)
for x in dec: ppl[x['書き換えた人']][x['区分']]+=1
order=sorted(ppl,key=lambda k:-sum(ppl[k].values()))
NAMES='--names' in sys.argv
alias={k:(k if NAMES else f'担当{chr(65+i)}') for i,k in enumerate(order)}
K=['訂正では説明しにくい：0・空に','訂正では説明しにくい：半分以上減','訂正では説明しにくい：少し減','訂正とみられる']
prow=[[alias[k],sum(ppl[k].values())]+[ppl[k][c] for c in K] for k in (order if NAMES else order[:5])]
rest=[] if NAMES else order[5:]
if rest: prow.append([f'ほか {len(rest)} 人',sum(sum(ppl[k].values()) for k in rest)]+[sum(ppl[k][c] for k in rest) for c in K])
days=[f(x['減らす前の値が残っていた日数']) for x in dec]
red={it:sum(f(x['減らす前の値'])-f(x['新しい値']) for x in dec if x['項目']==it) for it in ['面接数','採用数']}
dates=collections.Counter(x['日時(UTC)'][:10] for x in dec).most_common(4)
after=collections.Counter(x['その後'] for x in dec)
# expiry
ex=collections.defaultdict(lambda:[0,0.0])
for x in exp:
  d=f(x['差(月末−満了日)'])
  if d: ex[(x['満了の月'],x['項目'])][0]+=1; ex[(x['満了の月'],x['項目'])][1]+=d
exrows=[[m,ex[(m,'面接')][0],f"{int(ex[(m,'面接')][1]):+,}",ex[(m,'採用')][0],f"{int(ex[(m,'採用')][1]):+,}"] for m in sorted({k[0] for k in ex})]
exsum=[sum(f(x['差(月末−満了日)']) for x in exp if x['項目']==it) for it in ['面接','採用']]
exn=sum(1 for x in exp if f(x['差(月末−満了日)']))
oc=collections.Counter(x['区分'] for x in opt)
CSS=open(sys.argv[3]).read()
import json
co=json.load(open(f'{src}/carryover.json',encoding='utf-8'))
KC=['契約開始より前から数が入っていた取引で減らした','満了した契約の数を後から減らした','それ以外']
fd=co['first_drop']; fd_aug=sum(v for k,v in fd.items() if k.startswith('2026-08')); fd_left=sum(v for k,v in fd.items() if k.startswith('減らしていない'))
crow=[[alias[k]]+[co['by_person'].get(k,{}).get(c,0) for c in KC] for k in (order if NAMES else order[:5])]
if rest: crow.append([f'ほか {len(rest)} 人']+[sum(co['by_person'].get(k,{}).get(c,0) for k in rest) for c in KC])
inc2={r['月']:r for r in nn}
h=f'''<!doctype html><html lang="ja"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>面接数・採用数の月末集計</title><style>{CSS}</style></head><body><div class="page">
<h1>面接数・採用数の月末集計</h1>
<div class="sub">HubSpot リクロジ_納品管理の「面接数」「採用数」の変更履歴から、月ごとの値と書き換えを集計　｜　対象 2025/07〜2026/11/05　｜　2026/11/06 作成</div>

<h2>1. まとめ</h2>
<div class="kpi"><div><b>{fmt(f(inc[-3]['面接数の増え']))} / {fmt(f(inc[-3]['採用数の増え']))}</b><span>2026/09 の増え（面接 / 採用）</span></div><div><b>{fmt(f(inc[-2]['面接数の増え']))} / {fmt(f(inc[-2]['採用数の増え']))}</b><span>2026/10 の増え（面接 / 採用）</span></div><div><b>{len(dec)} 件</b><span>9/1 以降に値を減らした書き換え</span></div><div><b>{cat['訂正とみられる']} 件</b><span>うち訂正とみられるもの</span></div></div>
<ul>
<li>月末の値をそのまま足すと、契約が満了した取引が抜ける月に合計が下がる。満了による減りを除き、契約中の取引の「前の月末からの増え」を足した数を月の実績とした（3 章）。</li>
<li>2026/08 は合計が {fmt(f(inc[13]['面接数の増え']))}（面接）になる。2026/08/12 に 924 件の手での書き換えがあり、うち 598 件が値を減らしていた。この月の数字は使えない。</li>
<li>面接数・採用数は増えるだけの値のはずだが、9/1 以降に値を減らした書き換えが {len(dec)} 件ある。打ち間違いの訂正とみられるのは {cat['訂正とみられる']} 件だけで、残り {len(dec)-cat['訂正とみられる']} 件は訂正では説明しにくい（4 章）。</li>
<li>減らす前の値が残っていた日数は中央値 {statistics.median(days):.0f} 日、{sum(1 for d in days if d>30)} 件は 30 日を超えている。書き換えは 10/01 に {dates[0][1]} 件集中している。</li>
<li>月の初めに当月分を入れ直していたのではなかった。{co['kinds'][KC[0]]} 件は契約開始より前から数が入っていた取引での書き換えで、前の契約から引き継いだ数を今の契約の数に直したものとみられる（4-4）。</li>
</ul>

<h2>2. 対象と数え方</h2>
{table(['項目','内容'],[
['対象','パイプライン「リクロジ_納品管理」（21596025）の取引。項目は 面接数 <code>mensetu</code>・採用数 <code>syoudaku</code>。'],
['履歴','HubSpot のプロパティ履歴（<code>propertiesWithHistory</code>）。履歴は 2025/07/11 から。1 取引あたりの履歴は最大 14 件で、取得の上限で切れたものは無い。'],
['月末の値','各月の最終時刻（日本時間）より前の最後の値。'],
['数える取引','その月に契約が有効な取引（契約開始日〜契約満了日がその月に重なる）。面接数・採用数の両方が空の取引は除く。'],
['オプション取引','ステージ（1049738304・1281526627）か、取引名が「求人追加」「AirWork広告運用」「一次対応」「エントリーフォーム」で始まる取引。同じ会社の主契約にも数が入っている取引（数が違う 222・同じ 52、月×取引の件数）は主契約の数を使い、オプション側は数えない。オプションにしか数が無い 21 件と、会社の結び付きが無い 15 件は数えない。'],
['満了','満了した月は、満了日の値ではなくその月の月末の値を正とする（ユーザー決定）。満了の翌月に取引が抜けることによる減りは増えに含めない。'],
],[])}
<p class="note">オプション取引の判定は応募シートの判定（applicant_sheet.rs）と同じ。区分の内訳（月×取引）: {', '.join(f'{k} {v}' for k,v in oc.most_common())}。</p>

<h2>3. 月ごとの値</h2>
<h3>3-1. 月末の値の合計</h3>
<div class="fig">{bars(fin,[('面接数の合計',C1),('採用数の合計',C2)],0,5000,1000)}</div>
<p class="cap">図 1　契約中の取引の月末の値の合計。2026/11 は 11/05 時点。</p>
<p>合計は 2026/07 まで増え、2026/08 に半分近くまで下がっている。満了で抜けた取引と 8/12 の書き換えの両方が入っているため、この合計は月の実績としては使えない。</p>
<h3>3-2. 月の増え（満了による減りを除く）</h3>
<div class="fig">{bars(inc,[('面接数の増え',C1),('採用数の増え',C2)],-2000,1500,500)}</div>
<p class="cap">図 2　取引ごとの「前の月末からの増え」の合計。値を減らした書き換えはマイナスのまま足している。</p>
{table(['月','面接の増え','採用の増え','うち減った分（面接）','うち減った分（採用）','減りを 0 とした場合（面接 / 採用）'],
[[r['月']+('（途中）' if r['途中の月'] else ''),fmt(f(r['面接数の増え'])),fmt(f(r['採用数の増え'])),fmt(f(r['うち減った分(面接)'])),fmt(f(r['うち減った分(採用)'])),f"{fmt(f(inc2[r['月']]['面接数の増え(減りは0)']))} / {fmt(f(inc2[r['月']]['採用数の増え(減りは0)']))}"] for r in inc],{1,2,3,4,5})}
<p class="note">2025/07〜09 は履歴の始まりで、最初の値を基準にしただけの取引がある（2025/07 は 428 取引）。この期間の増えは小さく出る。</p>

<h3>3-3. 満了日の値と月末の値の差</h3>
<p>満了した取引の、満了日の値と満了した月の月末の値を比べた。満了日のあとに数を書き足している取引がある。差があったのは {exn} 件（取引×項目）、合計で面接 {int(exsum[0]):+,}、採用 {int(exsum[1]):+,}（月末のほうが多い）。</p>
{table(['満了の月','差がある取引（面接）','差（面接）','差がある取引（採用）','差（採用）'],exrows,{1,2,3,4})}
<p class="note">差 = 月末の値 − 満了日の値。2026/08 のマイナスは 8/12 の書き換えによる。</p>

<h2>4. 値を減らした書き換え（2026/09/01 以降）</h2>
<h3>4-1. 分け方</h3>
{table(['手順','内容'],[
['まとめる','同じ人が同じ取引・項目を 30 分以内に続けて書き換えたものは 1 回とみなす（「88 → 空 → 7」のような打ち直しを 1 回の変更として扱う）。'],
['訂正とみられる','減らす前の値が 4 日以内しか残っておらず、新しい値が、その前の値（減らす前の値に書き換える前の値）以上のもの。打ち間違いをすぐ直したとみる。'],
['訂正では説明しにくい','上に当たらないもの。減り方で「0・空にした」「半分以上減らした」「少し減らした」に分けた。'],
],[])}
<h3>4-2. 結果</h3>
<div class="kpi"><div><b>{cat[K[0]]}</b><span>0・空にした</span></div><div><b>{cat[K[1]]}</b><span>半分以上減らした</span></div><div><b>{cat[K[2]]}</b><span>少し減らした</span></div><div><b>{cat[K[3]]}</b><span>訂正とみられる</span></div></div>
<p>減らした数の合計は 面接 {int(red['面接数'])}、採用 {int(red['採用数'])}。その後に値が元の値以上に戻ったものが {after['後で元の値以上に戻った']} 件、戻らないが増えたものが {after['後で増えた']} 件、そのままが {after['']} 件。</p>
{table(['書き換えた人','件数','0・空に','半分以上減','少し減','訂正とみられる'],prow,{1,2,3,4,5})}
<p class="note">{'全員を件数の多い順に載せた。名前入りなので公開リポジトリに置かない。' if NAMES else '名前は伏せた（件数の多い順に 担当A〜）。名前付きの明細はリポジトリに置かず、下の「明細の置き場所」にある。'}</p>
{table(['日付（UTC）','件数'],[[d,n] for d,n in dates],{1})}

<h3>4-3. 読み方</h3>
<ul>
<li>減らす前の値が長く残っていたもの（30 日超が {sum(1 for d in days if d>30)} 件）は、打ち間違いの訂正とは考えにくい。</li>
<li>{alias[order[0]]+('さん' if NAMES else '')}・{alias[order[1]]+('さん' if NAMES else '')}の取引ごとの履歴を並べた。書き換えた後も値は契約の中で増え続けており、月ごとに 0 から入れ直してはいない（4-4）。</li>
<li>悪意かどうかは履歴だけでは判断できない。継続の取引を作るときに前の契約の数を入れるのか、0 から始めるのかを現場と確かめる必要がある。</li>
</ul>

<h3>4-4. 継続の取引に引き継いだ数</h3>
<p>継続（「サブスク継続」「再契約」）の取引は、契約開始日より前に作られ、そのときに前の契約の数がそのまま入っていることが多い。例: 前の契約の面接数が 13 なら、継続の取引にも開始前に 13 が入る。見た例では、入っていた数は前の契約の最後の値と同じだった。あとで今の契約の数（0 や 1）に直すと、値を減らした書き換えとして残る。</p>
{table(['書き換えた人','開始前から数が入っていた取引で減らした','満了した契約の数を後から減らした','それ以外'],crow,{1,2,3})}
<p class="note">9/1 以降に値を減らした書き換え {len(dec)} 件の内訳。「開始前から数が入っていた」= その項目で最初に 0 より大きい値が入った日が契約開始日より前。</p>
<p>2026 年に始まった継続の取引で、開始前に数が入っていたのは {co['pre'].get('継続|開始前',0)} 件（取引×項目。新規の取引では {co['pre'].get('新規|開始前',0)} 件）。このうち 8 月に直されたのが {fd_aug} 件（うち 8/12 の一斉の書き換えで {fd.get('2026-08-12',0)} 件）、10 月が {fd.get('2026-10',0)} 件。直されていないものが {fd_left} 件（うち契約中 {fd.get('減らしていない（契約中）',0)} 件）ある。</p>
<ul>
<li>8/12 の一斉の書き換え（924 件）のうち少なくとも {fd.get('2026-08-12',0)} 件は、この引き継いだ数を直したもの。</li>
<li>引き継いだ数は、入れた月に「増え」として数えられ、直した月に「減り」として数えられる。直していない取引では、前の契約の数が今の契約の実績に混ざったまま。</li>
</ul>

<h2>5. 明細の置き場所</h2>
{table(['ファイル','内容'],[
['docs/interview_hire_counts/scripts/','取得・集計のスクリプト（手順は README.md）'],
['/Users/s_fujimaki/orca/workspaces/HR_HR/deliverables/interview_hire_monthly/','取得したデータと明細の CSV（取引名・担当者名を含むのでリポジトリに入れない）'],
],[])}
</div></body></html>'''
open(out,'w').write(h)
