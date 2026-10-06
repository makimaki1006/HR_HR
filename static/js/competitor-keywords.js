(() => {
const NS='http://www.w3.org/2000/svg';
function node(svg,tag,attrs,text){const n=document.createElementNS(NS,tag);for(const [k,v] of Object.entries(attrs))n.setAttribute(k,v);if(text!==undefined)n.textContent=text;svg.append(n);return n;}
function draw(svg){
 if(document.body.classList.contains('pdf-document'))return;
 const box=svg.getBoundingClientRect();if(box.width<1||box.height<1)return;
 const data=JSON.parse(document.getElementById(svg.dataset.series).textContent);
 const w=box.width,h=box.height,left=Math.min(170,Math.max(86,w*.34)),right=48,plot=w-left-right;
 svg.replaceChildren();svg.setAttribute('viewBox',`0 0 ${w} ${h}`);
 node(svg,'title',{},data.title);
 const top=data.mode==='comparison'?42:20,bottom=30,step=(h-top-bottom)/Math.max(1,data.rows.length);
 const max=data.mode==='comparison'?100:Math.max(1,...data.rows.map(r=>r.all));
 const ticks=data.mode==='comparison'?(w<400?[0,50,100]:[0,25,50,75,100]):[0,max/2,max];
 for(const t of ticks){const x=left+plot*t/max;node(svg,'path',{d:`M${x} ${top} V${h-bottom}`,stroke:'#e1e8ec'});node(svg,'text',{x,y:h-8,'text-anchor':'middle','font-size':12,fill:'#536169'},data.mode==='comparison'?t+'%':String(Math.round(t)));}
 if(data.mode==='comparison'){
  node(svg,'rect',{x:left,y:8,width:12,height:8,fill:'#006666'});node(svg,'text',{x:left+17,y:17,'font-size':12},'全体');
  node(svg,'rect',{x:left+83,y:8,width:12,height:8,fill:'#4472c4'});node(svg,'text',{x:left+100,y:17,'font-size':12},'先頭 '+data.headN+'件');
 }
 data.rows.forEach((r,i)=>{
  const y=top+i*step,cy=y+step/2,labelLength=Math.max(4,Math.floor((left-14)/13));
  const label=[...r.word].length>labelLength?[...r.word].slice(0,labelLength-1).join('')+'…':r.word;
  const text=node(svg,'text',{x:left-10,y:cy+4,'text-anchor':'end','font-size':13,fill:'#20272c'},label);const title=document.createElementNS(NS,'title');title.textContent=r.word;text.append(title);
  const bars=data.mode==='comparison'?[[r.all===null?null:r.all/data.allN*100,'#006666','全体',cy-11],[r.head/data.headN*100,'#4472c4','先頭',cy+1]]:[[r.all,'#006666','全体',cy-10]];
  bars.forEach(([value,color,group,by])=>{
   if(value===null){node(svg,'text',{x:left+4,y:by+9,'font-size':11},'—');return;}
   const height=data.mode==='comparison'?9:20,width=plot*value/max;
   const bar=node(svg,'rect',{x:left,y:by,width,height,rx:2,fill:color,'data-word':r.word,'data-group':group,'data-value':value});
   const title=document.createElementNS(NS,'title');title.textContent=r.word+' / '+group+': '+(data.mode==='comparison'?value.toFixed(1)+'%':value+'件');bar.append(title);
   node(svg,'text',{x:left+width+4,y:by+height,'font-size':data.mode==='comparison'?11:13,fill:'#263b4a'},data.mode==='comparison'?value.toFixed(1)+'%':value+'件');
  });
 });
}
const observer=new ResizeObserver(entries=>entries.forEach(e=>draw(e.target)));
document.querySelectorAll('svg[data-series]').forEach(svg=>{observer.observe(svg);draw(svg)});
})();