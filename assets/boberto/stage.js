'use strict';
// Empty-library companion: the engine's peana (its own mirror canvas, same look as the dock) with gestures.
(function(){
 const post=m=>window.chrome?.webview?.postMessage(m);
 const box=document.getElementById('peana');
 let tries=0;
 function gestures(){try{return window.BOBERTO.listGestures().map(g=>g.id).filter(id=>id!=='duerme');}catch(e){return ['saluda'];}}
 function random(){const list=gestures();window.BOBERTO.peanaGesto(box,list[Math.floor(Math.random()*list.length)]);}
 function mount(){
  if(!window.BOBERTO||!window.BOBERTO.peana){if(++tries>200){post({type:'error',message:'Boberto engine missing.'});return;}setTimeout(mount,50);return;}
  if(!window.BOBERTO.peana(box,{plana:true,escalaMax:8,cada:6500})){if(++tries>200){post({type:'error',message:'Boberto stage did not fit.'});return;}setTimeout(mount,100);return;}
  setTimeout(()=>window.BOBERTO.peanaGesto(box,'saluda'),500);
  post({type:'ready'});
 }
 box.addEventListener('click',random);
 let t;window.addEventListener('resize',()=>{clearTimeout(t);t=setTimeout(()=>window.BOBERTO&&window.BOBERTO.peana(box,{plana:true,escalaMax:8,cada:6500}),120);});
 window.desktopStage={applyTheme(colors,theme,motion){const r=document.documentElement;for(const [n,v] of Object.entries(colors))r.style.setProperty(n,v);r.setAttribute('data-theme',theme);r.setAttribute('data-animations',motion?'on':'off');document.dispatchEvent(new Event('bzp:theme-preview'));},wave(){return window.BOBERTO.peanaGesto(box,'saluda');}};
 // Studio writes bzp_* keys from another WebView of the same profile: reload to pick the new look.
 let rt;window.addEventListener('storage',e=>{if(e.key!==null&&!/^bzp_(boberto|mascota)_/.test(e.key))return;clearTimeout(rt);rt=setTimeout(()=>location.reload(),400);});
 if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',()=>setTimeout(mount,0),{once:true});else setTimeout(mount,0);
})();
