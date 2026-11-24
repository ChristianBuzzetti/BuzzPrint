'use strict';
const POSES_ATLAS=[
 'base','blink','wink','lookL',
 'lookR','lookU','happy','waveUp',
 'waveDown','festejaSalto','festejaPuno','danceR',
 'danceL','think','coffeeHold','coffeeSip'
];
function collectLook(){
 const B=window.BOBERTO;
 const ch=B.getCharacter()||'';
 const hat=B.getHat()||'samurai';
 let label='';
 if(ch){const c=B.listCharacters().find(x=>x.id===ch);if(c)label=c.name||'';}
 else{const h=B.listHats().find(x=>x.id===hat);if(h)label=h.name||'';}
 return {
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
}
function bakeSheet(){
 const B=window.BOBERTO;
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
 return sheet.toDataURL('image/png');
}
function bootStudio(){
const studio=window.BOBERTO_ESTUDIO.montar({escena:document.getElementById('stage'),tabs:document.getElementById('tabs'),paneles:document.getElementById('panels'),fondo:document.getElementById('background'),marco:document.getElementById('frame'),rotulo:document.getElementById('look'),fabrica:document.getElementById('factory'),volver:document.getElementById('restore'),persistir:true,igualarAlto:false,escala:()=>{const stage=document.getElementById('stage');return Math.round(Math.max(7,Math.min(15,stage.clientHeight/30,stage.clientWidth/36)));}});
if(!studio)throw new Error('Boberto Studio did not initialize.');
window.desktopStudio={
 applyTheme(colors,theme,motion){const root=document.documentElement;for(const [name,value] of Object.entries(colors))root.style.setProperty(name,value);root.setAttribute('data-theme',theme);root.setAttribute('data-animations',motion===false?'off':'on');document.dispatchEvent(new Event('bzp:theme-preview'));scheduleFrame();},
 facts(){const B=window.BOBERTO;return {ready:!!studio,gestures:B.listGestures().length,accessories:B.listHats().length,skins:B.listCharacters().length,backgrounds:B.listFondos().length,borders:B.listBordes().length,skin:B.getSkin(),hat:B.getHat(),character:B.getCharacter(),panels:document.querySelectorAll('#tabs [role=tab]').length};},
 teardown(){clearTimeout(frameTimer);studio.desmontar();window.BOBERTO.stopPreview();},
 reset(){window.BOBERTO.resetLook();exportFrame();}
};
let frameTimer;
function exportFrame(){
 const B=window.BOBERTO,canvas=document.createElement('canvas');
 B.renderPreview(canvas,4);const png=canvas.toDataURL('image/png');B.stopPreview();
 window.chrome?.webview?.postMessage({type:'avatar',png});
 try{
  const look=collectLook();
  const sheet=bakeSheet();
  window.chrome?.webview?.postMessage({type:'native-sync',look,sheet});
 }catch(e){}
}
function scheduleFrame(){clearTimeout(frameTimer);frameTimer=setTimeout(exportFrame,350);}
['boberto-skin-changed','boberto-character-changed','boberto-fx-changed','boberto-fondo-changed','boberto-borde-changed'].forEach(name=>document.addEventListener(name,scheduleFrame));
window.addEventListener('pagehide',()=>window.desktopStudio.teardown(),{once:true});
// Choice cooldown: a look choice (background, accessory, border...) applies at once; further choices wait
// COOLDOWN_MS so rapid clicking cannot stack exports. Hover previews stay available.
const COOLDOWN_MS=1500;let coolUntil=0,coolTimer;
const panels=document.getElementById('panels');
function choiceOf(target){const chip=target instanceof Element?target.closest('button.bes-chip[aria-pressed]'):null;return chip&&panels.contains(chip)?chip:null;}
panels.addEventListener('click',event=>{
 const chip=choiceOf(event.target);if(!chip)return;
 if(performance.now()<coolUntil){event.preventDefault();event.stopImmediatePropagation();return;}
 if(chip.getAttribute('aria-pressed')==='true')return;
 coolUntil=performance.now()+COOLDOWN_MS;document.documentElement.setAttribute('data-cooldown','');
 clearTimeout(coolTimer);coolTimer=setTimeout(()=>document.documentElement.removeAttribute('data-cooldown'),COOLDOWN_MS);
},true);
window.desktopStudio.cooldownMs=COOLDOWN_MS;
window.chrome?.webview?.postMessage({type:'ready',facts:window.desktopStudio.facts()});scheduleFrame();
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',bootStudio,{once:true});else bootStudio();
