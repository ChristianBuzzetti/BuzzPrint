'use strict';
// One-shot hidden sync: reads the shared WebView2 localStorage (e.g. after BuzzPrint .NET Studio changed the look),
// bakes the 4x4 high-DPI pose atlas + look JSON, posts them to Rust, and signals sync-done so WebView2 can close at once.
(function(){
 const POSES_ATLAS=[
  'base','blink','wink','lookL',
  'lookR','lookU','happy','waveUp',
  'waveDown','festejaSalto','festejaPuno','danceR',
  'danceL','think','coffeeHold','coffeeSip'
 ];
 function run(){
  const B=window.BOBERTO;
  if(!B||typeof B.renderPose!=='function'){
   window.chrome?.webview?.postMessage({type:'error',message:'Boberto engine missing in sync.'});
   return;
  }
  const ch=B.getCharacter()||'';
  const hat=B.getHat()||'samurai';
  let label='';
  if(ch){const c=B.listCharacters().find(x=>x.id===ch);if(c)label=c.name||'';}
  else{const h=B.listHats().find(x=>x.id===hat);if(h)label=h.name||'';}
  const look={
   skin:B.getSkin()||'auto',
   skin_color:B.getSkinColor()||'#000000',
   eye:B.getEyeColor()||'auto',
   hat,
   hat_color:B.getHatColor()||'#2a2c34',
   hat_color2:B.getHatColor2()||'auto',
   character:ch,
   fx_body:B.getFx('body')||[],
   fx_acc:B.getFx('acc')||[],
   glow_body:B.getGlow('body')||'auto',
   fondo:B.getFondo()||'ninguno',
   fondo_op:B.getFondoOpacidades()||{},
   borde:B.getBorde()||'ninguno',
   label
  };
  const CW=160,CH=168,COLS=4,ROWS=4;
  const sheet=document.createElement('canvas');
  sheet.width=CW*COLS;sheet.height=CH*ROWS;
  const ctx=sheet.getContext('2d');
  ctx.imageSmoothingEnabled=true;
  ctx.imageSmoothingQuality='high';
  const cell=document.createElement('canvas');
  for(let i=0;i<POSES_ATLAS.length;i++){
   B.renderPose(cell,POSES_ATLAS[i],8);
   const col=i%COLS,row=(i/COLS)|0;
   ctx.drawImage(cell,0,0,cell.width,cell.height,col*CW,row*CH,CW,CH);
  }
  const av=document.createElement('canvas');
  B.renderPreview(av,4);
  const avatarPng=av.toDataURL('image/png');
  B.stopPreview();
  window.chrome?.webview?.postMessage({type:'avatar',png:avatarPng});
  window.chrome?.webview?.postMessage({type:'native-sync',look,sheet:sheet.toDataURL('image/png')});
  window.chrome?.webview?.postMessage({type:'sync-done'});
 }
 if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',()=>setTimeout(run,0),{once:true});else setTimeout(run,0);
})();
