// 旧画面 templates/tabs/sales_kpi.html (origin/main ad7d918) の JS から、純関数の行を
// **そのまま** 切り出したもの (scratchpad の make_legacy.py が行番号で切り出す。手で書き写していない)。
// calc.ts の同名関数が同じ入力→同じ出力になることを calc.legacy.test.ts で確かめるためだけに置く。
// 画面からは import しない。ESLint の対象外 (.js は eslint.config.js の files に無い)。

// --- L331
export const WD=['日','月','火','水','木','金','土'];
// --- L332-333
export const fmt=n=>(n==null?'—':n.toLocaleString('ja-JP'));
export const pct=n=>(n==null?'—':n.toFixed(1)+'%');
// --- L344-345
export const md=s=>{const d=new Date(s+'T00:00:00+09:00');return (d.getMonth()+1)+'/'+d.getDate();};
export const wd=s=>WD[new Date(s+'T00:00:00+09:00').getDay()];
// --- L363 (TODAY は関数の外の const なので、引数で渡す factory に包む)
export function makeAgo(TODAY){
  const ago=s=>Math.round((new Date(TODAY+'T00:00:00+09:00')-new Date(s+'T00:00:00+09:00'))/86400000);
  return ago;
}
// --- L399-407 (hidden は関数の外の Set なので、引数で渡す factory に包む)
export function makeSumIf(hidden){
function sumIf(byPerson, ok){
  const o={};
  for(const id in (byPerson||{})){
    if(hidden.has(id)||!ok(id)) continue;
    const v=byPerson[id];
    for(const k in v) o[k]=(o[k]||0)+v[k];
  }
  return o;
}
  return sumIf;
}
// --- L522-526
export function avgLine(ab, total, unit){
  if(!ab||!ab.n) return '';
  return ab.label+' '+(total/ab.n).toFixed(1)+(unit||'件')+
         '<span style="opacity:.75">（'+ab.n+'名）</span>';
}
// --- L559-566
export function wowEl(now,prev,unit,invert){
  if(prev==null) return '';
  const d=now-prev; if(!d) return '<div class="wow" style="color:var(--faint)">先週と同じ</div>';
  const good=invert?d<0:d>0;
  const col=good?'var(--ok)':'var(--alert)';
  return '<div class="wow" style="color:'+col+'">'+(d>0?'▲':'▼')+Math.abs(d).toLocaleString('ja-JP')+
    (unit||'')+' <span style="color:var(--faint)">先週 '+fmt(prev)+(unit||'')+'</span></div>';
}
// --- L1149-1150
export const growText=n=>(n==null?'—':(n===0?'±0':(n>0?'+':'')+fmt(n)));
export const growColor=n=>(n==null||n===0?'var(--faint)':(n>0?'var(--ok)':'var(--alert)'));
// --- L1247-1282 (LS_LISTS / LS_KINDS は関数の外の const なので、引数で渡す factory に包む)
export function makeLs(LS_LISTS){
const LS_KINDS=['アクティブ','保管'];
const escS=v=>String(v==null?'':v).replace(/[&<>"']/g,
  ch=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch]));
const lsGet=(c,b)=>(c||{})[b]||0;
// 2つの数え上げ（{帯: 件数}）を足す。
const lsAdd=(a,b)=>{const o=Object.assign({},a);
  Object.keys(b||{}).forEach(k=>{o[k]=(o[k]||0)+b[k];}); return o;};
// 割合。母数が 0 なら出さない（0% と書くと「無い」と「数えられない」が区別できない）。
const lsPct=(n,d)=>d?pct(n/d*100):'—';

// 内訳の名前。リストをまたいで同じものを1行にまとめる（並びは最初に出てきた順）。
// どのリストでも同じ行の並びにするので、リクロジと大分の表を上下で見比べられる。
const lsNames=kind=>{
  const names=[];
  LS_LISTS.forEach(l=>(l.groups||[]).forEach(g=>{
    if(g.kind===kind&&!names.includes(g.name)) names.push(g.name);}));
  return names;
};
// 1つのリストの行。内訳 → 区分の計 → その他。`sub` の行が「区分の計」と「その他」で、
// この3つ（アクティブの計・保管の計・その他）を足すとリストの全体になる。
function lsRows(l){
  const rows=[];
  LS_KINDS.forEach(kind=>{
    const names=lsNames(kind);
    if(!names.length) return;
    let n={}, named={};
    names.forEach(name=>{
      const g=(l.groups||[]).find(x=>x.kind===kind&&x.name===name)||{};
      rows.push({label:escS(name), note:kind, n:g.counts||{}, named:g.named||{}});
      n=lsAdd(n,g.counts); named=lsAdd(named,g.named);
    });
    rows.push({label:kind+'の計', sub:true, key:kind, n:n, named:named});
  });
  rows.push({label:'その他', note:'区分シートに書かれていない人・担当者なし',
    sub:true, key:'その他', n:l.other||{}, named:l.other_named||{}});
  return rows;
}
  return {escS, lsGet, lsAdd, lsPct, lsNames, lsRows};
}
