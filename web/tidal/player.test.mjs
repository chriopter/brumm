import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const source = (await readFile(new URL('./player.js',import.meta.url),'utf8')).replace(/^import .*;\n/gm,'');
const tick = () => new Promise(resolve=>setImmediate(resolve));
async function fixture() {
  const events=new EventTarget();let active=null,position=0,state='IDLE',next;
  let gate=null,quality="HIGH",loadedQuality="",rejectLossless=false,presentation='FULL';
  const Player={events,getMediaProduct:()=>active,getMediaElement:()=>null,getAssetPosition:()=>position,getPlaybackState:()=>state,
    getPlaybackContext:()=>({actualAudioQuality:'LOSSLESS',actualDuration:240,codec:'flac',bitDepth:16,sampleRate:44100,actualAssetPresentation:presentation}),
    async load(product,pos=0){ if(rejectLossless && ["HI_RES_LOSSLESS","LOSSLESS"].includes(quality)) {const error=new Error("unavailable");error.errorId="PEContentNotAvailableForSubscription";throw error;} if(gate) {await gate;gate=null;} active=product;loadedQuality=quality;position=pos;state='NOT_PLAYING';events.dispatchEvent(new CustomEvent('media-product-transition',{detail:{mediaProduct:product}}));},
    async play(){state='PLAYING';},async pause(){state='NOT_PLAYING';},async seek(pos){position=pos;},async reset(){active=null;state='IDLE';},
    async setNext(product){next=product;},setCredentialsProvider(){},setEventSender(){},bootstrap(){},setAudioAdaptiveBitrateStreaming(){},setTransitionMode(){},setStreamingWifiAudioQuality(value){quality=value;},setLoudnessNormalizationMode(){},setVolumeLevel(){}
  };
  const window={BRUMM_CFG:{version:'fixture'}};
  async function fetch(url,request) {
    const body=JSON.parse(request.body);
    return {ok:true,async json(){return url.endsWith('/credentials') ? {token:'fixture',clientId:'fixture',userId:'fixture'} : body.ids.map(id=>({id,title:'Track '+id,duration:240}));}};
  }
  new Function('Player','Events','window','location','fetch','navigator',source)(Player,{async init(){}},window,{pathname:'/private'},fetch,{userAgent:'Chrome/1.0'});
  await tick();
  return {B:window.brumm,Player,previewOnly(){presentation="PREVIEW";},rejectLossless(){rejectLossless=true;},get quality(){return quality;},get loadedQuality(){return loadedQuality;},get next(){return next;},hold(){let release;gate=new Promise(resolve=>release=resolve);return release;}};
}
test('duplicate tracks retain queue positions and actual audio quality',async()=>{
  const f=await fixture();await f.B.playIds(['1','2','1'],'1','album');
  assert.deepEqual(JSON.parse(f.B.queue(20)).items.map(t=>t.id),['1','2','1']);
  assert.equal(f.next.referenceId,'1:1');await f.B.jump(2);
  const state=JSON.parse(f.B.state());assert.equal(state.index,2);assert.equal(state.audioQuality,'LOSSLESS');assert.equal(state.sampleRate,44100);
});
test('stale transitions cannot overwrite a new queue',async()=>{
  const f=await fixture();await f.B.playIds(['1','2'],'1','old');const old=f.Player.getMediaProduct();
  await f.B.playIds(['3'],'3','new');f.Player.events.dispatchEvent(new CustomEvent('media-product-transition',{detail:{mediaProduct:old}}));
  assert.equal(JSON.parse(f.B.state()).id,'3');assert.equal(JSON.parse(f.B.state()).source,'new');
});
test('a superseded asynchronous load never starts playback',async()=>{
  const f=await fixture();const release=f.hold();const first=f.B.playIds(['1'],'1','old');await tick();
  const second=f.B.playIds(['2'],'2','new');release();await first;await second;
  assert.equal(JSON.parse(f.B.state()).id,'2');assert.equal(f.Player.getMediaProduct().productId,'2');
});
test('preview restores the queue, position and pause state',async()=>{
  const f=await fixture();await f.B.playIds(['1','2'],'1','album',42);await f.B.pause();
  await f.B.preview('3');assert.equal(JSON.parse(f.B.state()).preview.title,'Track 3');
  await f.B.stopPreview();const state=JSON.parse(f.B.state());assert.equal(state.id,'1');assert.equal(state.pos,42);assert.equal(state.playing,false);assert.equal(state.length,2);
});
test('repeat and insertion update the prepared successor',async()=>{
  const f=await fixture();await f.B.playIds(['1','2'],'1','album');await f.B.enqueue(['3'],true);
  assert.equal(f.next.productId,'3');await f.B.repeat(1);assert.equal(f.next.productId,'1');
  await f.B.repeat(2);await f.B.jump(2);assert.equal(f.next.productId,'1');
});

test('quality fallback preserves the request and uses an available tier',async()=>{
  const f=await fixture();f.rejectLossless();await f.B.audioOptions('HI_RES_LOSSLESS','TRACK');
  await f.B.playIds(['1'],'1','album');
  assert.equal(JSON.parse(f.B.state()).requestedQuality,'HI_RES_LOSSLESS');
  assert.equal(f.loadedQuality,'HIGH');assert.equal(JSON.parse(f.B.state()).err,'');
});

test('preview-only entitlement is reported and cannot masquerade as full playback',async()=>{
  const f=await fixture();f.previewOnly();await f.B.playIds(['1','2'],'1','album');
  const state=JSON.parse(f.B.state());assert.equal(state.presentation,'PREVIEW');
  assert.equal(state.playing,false);assert.match(state.err,/only a preview/);
  await f.B.toggle();assert.equal(JSON.parse(f.B.state()).playing,false);
});
