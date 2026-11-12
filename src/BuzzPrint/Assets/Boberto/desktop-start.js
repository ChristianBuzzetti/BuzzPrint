// BuzzPrint default look (samurai, black body, blue eyes, Mirai night, crossed frame). Seeded once per profile and
// only for keys the person has not chosen yet, so Studio choices always win.
(function(){try{
 // Keys saved under an earlier prefix (xx_boberto_*, xx_mascota_*) move to bzp_ once, so an existing look is kept.
 if(localStorage.getItem('bzp_prefix_v1')===null){
  const old=[];
  for(let i=0;i<localStorage.length;i++){const k=localStorage.key(i);const m=k&&k.match(/^([a-z]+)_((?:boberto|mascota)_[a-z_0-9]+)$/);if(m&&m[1]!=='bzp')old.push([k,'bzp_'+m[2]]);}
  for(const [from,to] of old){if(localStorage.getItem(to)===null)localStorage.setItem(to,localStorage.getItem(from));localStorage.removeItem(from);}
  localStorage.setItem('bzp_prefix_v1','1');
 }
 if(localStorage.getItem('buzzprint_defaults_v1')===null){
  const d={bzp_boberto_skin:'custom',bzp_boberto_skin_color:'#000000',bzp_boberto_eye:'#2684ff',bzp_boberto_hat:'samurai',bzp_boberto_fondo:'mirai-noche',bzp_boberto_borde:'cruce',bzp_boberto_migrated:'1',bzp_boberto_fx_acc_migrated_v1:'1'};
  for(const k in d)if(localStorage.getItem(k)===null)localStorage.setItem(k,d[k]);
  localStorage.setItem('buzzprint_defaults_v1','1');
 }}catch(e){}})();
window.BOBERTO_HOST={tips:[]};
window.desktopErrors=[];
// ResizeObserver loop notices are benign browser warnings raised as error events; they are not script failures.
window.addEventListener('error',event=>{const message=String(event.message||'Boberto script error').slice(0,500);if(/^ResizeObserver loop/.test(message))return;window.desktopErrors.push(message);window.chrome?.webview?.postMessage({type:'error',message});});
