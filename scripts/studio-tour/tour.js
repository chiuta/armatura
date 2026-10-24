// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
// ---- guided demo: builds the walking person step by step; each step loads that stage of the project (via /api/op, replace-all) ----
const DM={active:false,i:0,orig:null,data:null,tab:'t',seen:new Set(),saveTitle:''};
const DM_BONES=['skeleton'], DM_ANIM=['legs','swing','trunk','face','ground','check'], DM_EXPORT='export';
function dmData(){ if(!DM.data)DM.data=JSON.parse($('demo-tour').textContent); return DM.data; }
async function dmGunzip(b){ const u=Uint8Array.from(atob(b),c=>c.charCodeAt(0)); return await new Response(new Blob([u]).stream().pipeThrough(new DecompressionStream('gzip'))).text(); }
async function dmLoad(){ const D=dmData(); if(D.ready)return; for(const s of D.steps){ if(s.zp){ s.project=await dmGunzip(s.zp); delete s.zp; } } for(const s of D.steps){ if(s.same)s.project=D.steps.find(x=>x.id===s.same).project; } D.ready=true; }
function dmOpen(on){ const b=$('dmp'); const open=on===undefined?b.style.display!=='block':on; b.style.display=open?'block':'none'; if(open&&DM.active)dmFill(); }
function dmSet(value){ return api('/api/op',{op:'set',path:'',value}); }
async function dmApply(value){ const r=await dmSet(value); if(r&&r.ok===false){ toast((r.errors||['could not load the project']).join('; '),true); return false; }
 S=r; frame=0; $('tl').max=Math.max(0,(S.frames||1)-1); await refresh(true); return true; }
function dmLine(parent,cls,txt){ const s=document.createElement('span'); if(cls)s.className=cls; s.textContent=txt; parent.appendChild(s); }
function dmTerm(){ const st=dmData().steps[DM.i], t=$('dm-term'); t.textContent='';
 if(DM.tab==='s'&&st.files.length){ st.files.forEach((f,k)=>{ if(k)t.appendChild(document.createTextNode('\n\n')); dmLine(t,'pr','$'); t.appendChild(document.createTextNode(' cat '+f.name+'\n')); dmLine(t,'out',f.text); }); return; }
 st.commands.forEach(c=>{ dmLine(t,'pr','$'); t.appendChild(document.createTextNode(' '+c.cmd+'\n')); if(c.out)dmLine(t,'out',c.out+'\n'); }); }
function dmFill(){ const D=dmData(), st=D.steps[DM.i], n=D.steps.length;
 const tx=DM_TEXT[st.id]; $('dm-num').textContent='Step '+(DM.i+1)+' / '+n; $('dm-title').textContent=tx[0]; $('dm-desc').textContent=tx[1];
 [...$('dm-steps').children].forEach((b,k)=>{ b.setAttribute('aria-selected',k===DM.i); b.classList.toggle('done',DM.seen.has(k)&&k!==DM.i); });
 $('dm-tt').setAttribute('aria-selected',DM.tab==='t'); $('dm-ts').setAttribute('aria-selected',DM.tab==='s'); $('dm-ts').hidden=!st.files.length;
 if(!st.files.length&&DM.tab==='s'){ DM.tab='t'; $('dm-tt').setAttribute('aria-selected','true'); }
 $('dm-prev').disabled=DM.i===0; $('dm-next').disabled=DM.i===n-1; dmTerm();
 $('dm-note').textContent=st.id===DM_EXPORT?'Use the GIF, HTML player and Lottie buttons in the header: they export this project, the same files the commands above wrote.':'Real output of '+D.engine.replace(/ \(.*/,'')+'. The project shown in the editor is exactly what this step produced.'; }
async function dmGo(i){ const D=dmData(); i=Math.max(0,Math.min(D.steps.length-1,i)); const st=D.steps[i];
 if(!await dmApply(JSON.parse(st.project)))return; DM.i=i; DM.seen.add(i);
 $('bones').checked=DM_BONES.includes(st.id); sel=null; show(); bonesUi();
 playing=DM_ANIM.includes(st.id); $('play').textContent=playing?'Pause':'Play'; last=performance.now();
 document.querySelectorAll('header a.btn').forEach(a=>a.classList.toggle('dm-flash',st.id===DM_EXPORT));
 dmFill(); }
async function dmStart(){ try{ await dmLoad(); }catch(e){ toast('could not unpack the demo: '+e.message,true); return; }
 if(typeof COLLAB!=='undefined'&&COLLAB.on){ toast('Leave the collaboration session first: the demo replaces the whole project.',true); return; }
 DM.orig=await api('/api/project'); if(!DM.orig||DM.orig.ok===false){ toast('could not read the open project',true); return; }
 DM.active=true; DM.seen=new Set(); DM.saveTitle=$('save').title||''; $('save').disabled=true; $('save').title='disabled during the demo: press "Restore my project" first';
 $('dm-intro').style.display='none'; $('dm-run').style.display='block';
 $('dm-steps').innerHTML=dmData().steps.map((s,k)=>'<button type="button" role="tab" aria-selected="false" data-k="'+k+'" title="'+DM_TEXT[s.id][0]+'">'+(k+1)+'</button>').join('');
 await dmGo(0); }
async function dmRestore(){ if(!DM.active)return; playing=false; $('play').textContent='Play';
 document.querySelectorAll('header a.btn').forEach(a=>a.classList.remove('dm-flash'));
 if(!await dmApply(DM.orig))return; DM.active=false; $('save').disabled=false; $('save').title=DM.saveTitle;
 $('dm-run').style.display='none'; $('dm-intro').style.display='block'; toast('your project is back'); }
const DM_TEXT=/*DM_TEXT*/{};
$('demobtn').onclick=e=>{ e.stopPropagation(); dmOpen(); };
$('dm-x').onclick=()=>dmOpen(false);
$('dm-start').onclick=dmStart;
$('dm-prev').onclick=()=>dmGo(DM.i-1); $('dm-next').onclick=()=>dmGo(DM.i+1);
$('dm-steps').onclick=e=>{ const b=e.target.closest('button'); if(b)dmGo(+b.dataset.k); };
$('dm-tt').onclick=()=>{ DM.tab='t'; dmFill(); }; $('dm-ts').onclick=()=>{ DM.tab='s'; dmFill(); };
$('dm-restore').onclick=dmRestore;
Object.assign(CMD,{'demo.panel':()=>dmOpen()});
