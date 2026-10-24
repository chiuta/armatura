/* demo pas cu pas — redă randările REALE ale motorului (SVG) și ieșirea reală a comenzilor; nimic nu e desenat de mână. */
{
const D=JSON.parse(document.getElementById('demo-data').textContent);
const S=D.steps, N=S.length, FR=D.frames, FPS=D.fps;
const el={dv:$('#dv'),steps:$('#dsteps'),play:$('#dplay'),scrub:$('#dscrub'),fr:$('#dfr'),bar:$('#dv').parentElement.querySelector('.dbar'),num:$('#dnum'),title:$('#dtitle'),desc:$('#ddesc'),
  tabT:$('#dtabT'),tabS:$('#dtabS'),term:$('#dterm'),prev:$('#dprev'),next:$('#dnext'),auto:$('#dauto'),note:$('#dnote')};
const reduce=matchMedia('(prefers-reduced-motion: reduce)').matches;
const framesOf=s=>s.frames||(s.id==='export'?S.find(x=>x.id==='check').frames:null);
const esc=s=>s.replace(/[&<>]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;'}[c]));
let si=0,fi=0,playing=false,auto=false,tab='t',raf=0,last=0,typeTok=0,autoTimer=0,seen=new Set([0]),vis=false,intro=false;
const tt=k=>T[L][k];
const fmt=(k,o)=>tt(k).replace(/\{(\w+)\}/g,(_,x)=>o[x]);

function showFrame(i){const f=framesOf(S[si]);fi=i;
  el.dv.innerHTML=f?f[i]:S[si].still||'';
  el.scrub.value=i;el.fr.textContent=f?fmt('dFrame',{n:i,m:FR-1}):'';}
function stopAnim(){playing=false;cancelAnimationFrame(raf);el.play.textContent=tt('dPlay');el.play.setAttribute('aria-pressed','false')}
function startAnim(){const f=framesOf(S[si]);if(!f)return;playing=true;last=0;el.play.textContent=tt('dPause');el.play.setAttribute('aria-pressed','true');
  const tick=ts=>{if(!playing)return;if(!last)last=ts;const adv=Math.floor((ts-last)*FPS/1000);if(adv>0){last+=adv*1000/FPS;showFrame((fi+adv)%FR)}raf=requestAnimationFrame(tick)};raf=requestAnimationFrame(tick)}
function termHTML(s,upto){ // upto: număr de caractere de comandă afișate (typing); null = tot
  let left=upto==null?Infinity:upto,h='',cursor=false;
  for(const c of s.commands){
    const cmdTxt=c.cmd.length<=left?c.cmd:c.cmd.slice(0,left);left-=cmdTxt.length;
    h+='<span class="pr">$</span> '+esc(cmdTxt);
    if(cmdTxt.length<c.cmd.length){h+='<span class="cur"></span>';cursor=true;break}
    h+='\n'+(c.out?'<span class="out">'+esc(c.out)+'</span>\n':'');
  }
  if(!cursor&&upto!=null&&upto!==Infinity){}
  return h;}
function scriptHTML(s){return s.files.map(f=>'<span class="pr">$</span> cat '+esc(f.name)+'\n<span class="out">'+esc(f.text)+'</span>').join('\n\n')}
function renderTerm(upto){el.term.innerHTML=tab==='s'&&S[si].files.length?scriptHTML(S[si]):termHTML(S[si],upto);}
function typeIn(done,noType){const s=S[si],tok=++typeTok;const total=s.commands.reduce((a,c)=>a+c.cmd.length,0);
  if(reduce||tab!=='t'||noType){renderTerm(null);done&&done();return}
  let n=0;const step=Math.max(2,Math.ceil(total/40));
  const t=()=>{if(tok!==typeTok)return;n+=step;if(n>=total){renderTerm(null);done&&done();return}renderTerm(n);el.term.scrollTop=el.term.scrollHeight;setTimeout(t,22)};t()}
function setTab(k){tab=k;el.tabT.setAttribute('aria-selected',k==='t');el.tabS.setAttribute('aria-selected',k==='s');typeTok++;renderTerm(null)}
function labels(){el.tabT.textContent=tt('dTerm');el.tabS.textContent=tt('dScript');
  el.prev.textContent='← '+tt('dPrev');el.next.textContent=tt('dNext')+' →';el.auto.textContent=auto?tt('dStop'):tt('dAuto');
  el.play.textContent=playing?tt('dPause'):tt('dPlay');el.note.textContent=fmt('dNote',{engine:D.engine.replace(/ \(.*/,'')});
  el.steps.querySelectorAll('button').forEach((b,i)=>b.querySelector('span').textContent=tt('dS'+(i+1)));
  el.num.textContent=fmt('dStep',{n:si+1,m:N});el.title.textContent=tt('dS'+(si+1));el.desc.textContent=tt('dD'+(si+1));}
function go(i,noType){i=Math.max(0,Math.min(N-1,i));const wasPlaying=playing;stopAnim();si=i;seen.add(i);
  el.steps.querySelectorAll('button').forEach((b,k)=>{b.setAttribute('aria-selected',k===i);b.classList.toggle('done',seen.has(k)&&k!==i)});
  const sel=el.steps.children[i];if(sel&&el.steps.scrollWidth>el.steps.clientWidth){el.steps.scrollTo({left:sel.offsetLeft-el.steps.clientWidth/2+sel.clientWidth/2,behavior:reduce?'auto':'smooth'})}
  const f=framesOf(S[i]);el.bar.classList.toggle('off',!f);el.scrub.disabled=!f;
  el.tabS.hidden=!S[i].files.length;if(!S[i].files.length&&tab==='s'){tab='t';el.tabT.setAttribute('aria-selected','true');el.tabS.setAttribute('aria-selected','false')}
  el.prev.disabled=i===0;el.next.disabled=i===N-1;labels();showFrame(0);
  typeIn(()=>{if(f&&!reduce){vis?startAnim():(el.play.dataset.resume='1')}if(auto)scheduleAuto()},noType);
  if(f&&reduce&&wasPlaying)startAnim();}
function scheduleAuto(){clearTimeout(autoTimer);if(!auto)return;const f=framesOf(S[si]);autoTimer=setTimeout(()=>{if(!auto)return;if(si>=N-1){setAuto(false);return}go(si+1)},f?3200:2400)}
function setAuto(on){auto=on;clearTimeout(autoTimer);labels();if(on){if(si>=N-1){seen=new Set();go(0)}else scheduleAuto()}}

// construcție
el.steps.innerHTML=S.map((s,i)=>`<button type="button" role="tab" aria-selected="false" data-step="${i}"><i>${i+1}</i><span></span></button>`).join('');
el.steps.onclick=e=>{const b=e.target.closest('button');if(b){auto=false;go(+b.dataset.step)}};
el.prev.onclick=()=>{auto=false;go(si-1)};el.next.onclick=()=>{auto=false;go(si+1)};
el.auto.onclick=()=>setAuto(!auto);
el.play.onclick=()=>{playing?stopAnim():startAnim()};
el.scrub.oninput=()=>{stopAnim();showFrame(+el.scrub.value)};
el.tabT.onclick=()=>setTab('t');el.tabS.onclick=()=>setTab('s');
document.addEventListener('keydown',e=>{if(!el.steps.closest('.demo').contains(document.activeElement))return;if(e.key==='ArrowRight'&&e.target.tagName!=='INPUT'){auto=false;go(si+1)}if(e.key==='ArrowLeft'&&e.target.tagName!=='INPUT'){auto=false;go(si-1)}});
// pornește animația doar când secțiunea e vizibilă (și oprește-o când iese din ecran)
const demoIO=new IntersectionObserver(es=>es.forEach(e=>{vis=e.isIntersecting;if(vis&&!intro){intro=true;go(0);return}if(!e.isIntersecting){if(playing){stopAnim();el.play.dataset.resume='1'}}else if(el.play.dataset.resume==='1'){delete el.play.dataset.resume;startAnim()}}),{threshold:.15});demoIO.observe($('#demoapp'));
const prevLang=onLang;onLang=async()=>{prevLang&&await prevLang();labels()};
go(0,true);
}
