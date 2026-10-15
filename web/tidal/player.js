import * as Player from '@tidal-music/player';
import * as Events from '@tidal-music/event-producer';

const CFG = window.BRUMM_CFG;
const B = window.brumm = { ready:false, err:'', needsAuth:false, source:'', rev:0 };
let waiter, flush;
function touch() { B.rev++; if (waiter && !flush) flush = setTimeout(() => {flush=0; waiter?.();},30); }
function fail(error) {
  // A failed SDK load can pause an outstanding play promise. Keep its original cause.
  if (error?.name === 'AbortError' && B.err) return;
  const messages={PEContentNotAvailableForSubscription:'This track is unavailable for your Tidal subscription.',PEContentNotAvailableInLocation:'This track is unavailable in your region.',PEMonthlyStreamQuotaExceeded:'Your Tidal streaming limit has been reached.',PENetwork:'Tidal could not connect. Check your network and retry.',PENotAllowed:'Tidal did not allow playback for this account or application.',PERetryable:'Tidal playback was interrupted. Try again.'};
  B.err=messages[error?.errorId] || error?.message || 'Tidal playback failed. Try again.';
  if (error?.errorId && /^[A-Za-z0-9_]+$/.test(error.errorId)) {
    const code = /^[A-Za-z0-9_]+$/.test(error.errorCode||'') ? '/'+error.errorCode : '';
    B.err += ' ('+error.errorId+code+')';
  }
  if (/401|unauthorized|authorization/i.test(B.err)) B.needsAuth=true;
  touch();
}
async function bridge(route, body={}) {
  const response = await fetch(location.pathname + '/' + route, {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body),signal:AbortSignal.timeout(30000)});
  if (!response.ok) { if(response.status===401){B.needsAuth=true;touch();} const detail = await response.json().catch(()=>({})); throw new Error(response.status === 401 ? 'Tidal authorization expired' : detail.error || 'Tidal request failed'); }
  return response.json();
}
let token = '', listeners = [];
const credentials = {
  bus(callback) { listeners.push(callback); },
  async getCredentials(subStatus) {
    const result = await bridge('credentials', {rejected:subStatus ? token : null});
    const changed = result.token !== token; token = result.token;
    if (changed) for (const callback of listeners) callback(new CustomEvent('CredentialsUpdatedMessage',{detail:{type:'CredentialsUpdatedMessage',payload:result}}));
    return result;
  }
};
let queue=[], index=-1, generation=0, volume=1, repeat=0, shuffle=false, preview=null;
let requestedQuality=CFG.quality||'HIGH', loading=false;
let cache = new Map();
let command = Promise.resolve();
function serial(fn) { command = command.catch(()=>{}).then(fn).catch(fail); return command; }
function product(i,gen=generation) { return {productId:queue[i]?.id,productType:'track',referenceId:gen+':'+i,sourceId:B.source || 'brumm',sourceType:'brumm'}; }
function following() { if (repeat === 1) return index; if (index+1 < queue.length) return index+1; return repeat===2 && queue.length ? 0 : -1; }
async function qualityAttempt(action) {
  const tiers=['HI_RES_LOSSLESS','LOSSLESS','HIGH','LOW'];
  const candidates=tiers.slice(Math.max(0,tiers.indexOf(requestedQuality)));
  let error;
  for (const quality of candidates) {
    Player.setStreamingWifiAudioQuality(quality);
    try { return await action(); }
    catch(e) {
      error=e;
      if (!['PEContentNotAvailableForSubscription','PEContentNotAvailableInLocation'].includes(e?.errorId) && !/^S3/.test(e?.errorCode||'')) throw e;
    }
  }
  throw error;
}
async function prepare() {
  const i=following();
  try { await qualityAttempt(()=>Player.setNext(i>=0 ? product(i) : undefined)); }
  catch { /* Loading the next track explicitly reports its error if it remains unavailable. */ }
}
async function metadata(ids) {
  const missing = [...new Set(ids.filter(id=>!cache.has(id)))];
  for (let i=0;i<missing.length;i+=20) {
    const tracks = await bridge('tracks',{ids:missing.slice(i,i+20)});
    for (const track of tracks) cache.set(track.id,track);
  }
  return ids.map(id=>cache.get(id)).filter(Boolean);
}
function requireFull() {
  if (Player.getPlaybackContext()?.actualAssetPresentation === 'PREVIEW') {
    throw new Error('Tidal provided only a preview for this account or connection. Full-track playback is unavailable.');
  }
}
async function start(i, pos=0, gen=generation) {
  if (gen !== generation || !queue[i]) return;
  await metadata(queue.slice(i,i+2).map(t=>t.id));
  if (gen !== generation) return;
  for (let j=i;j<Math.min(queue.length,i+2);j++) queue[j]=cache.get(queue[j].id) || queue[j];
  index=i; B.err=''; B.needsAuth=false; touch();
  loading=true;
  try {await qualityAttempt(()=>Player.load(product(i,gen),Math.max(0,pos)));} finally {loading=false;}
  if (gen !== generation) return;
  requireFull(); await Player.play(); await prepare();
}
B.playIds = (ids, startId, source, pos=0) => {
  const gen = ++generation; preview=null;
  B.source=source; B.err=''; touch();
  return serial(async()=> {
    const at = Math.max(0,ids.indexOf(startId));
    await metadata(ids.slice(at,at+2));
    if (gen!==generation) return;
    queue=ids.map(id=>cache.get(id) || {id,title:'',duration:0}); index=at;
    await start(index,pos,gen);
  });
};
B.queue = async n => {
  const gen = generation, at = index;
  const items = queue.slice(Math.max(0,at),Math.max(0,at)+Math.min(500,Math.max(0,n)));
  await metadata(items.map(t=>t.id));
  if (gen !== generation || at !== index) return B.queue(n);
  const tracks = items.map(t=>cache.get(t.id) || t);
  for (let i=0;i<tracks.length;i++) queue[Math.max(0,at)+i] = tracks[i];
  return JSON.stringify({pos:at,items:tracks});
};
B.jump = i => serial(()=>start(i));
B.next = () => serial(async()=> { const i=following(); if(i>=0) await start(i); else {await Player.pause();touch();} });
B.prev = () => serial(async()=> { if(Player.getAssetPosition()>3) {await Player.seek(0);touch();} else await start(Math.max(0,index-1)); });
B.toggle = () => serial(async()=> {if(Player.getPlaybackState()==='PLAYING' || Player.getPlaybackState()==='STALLED') await Player.pause(); else {requireFull();await Player.play();}touch();});
B.play = () => serial(async()=>{requireFull();await Player.play();touch();});
B.pause = () => serial(async()=>{await Player.pause();touch();});
B.seek = pos => serial(async()=> {await Player.seek(Math.max(0,pos));touch();});
B.volume = v => {volume=Math.max(0,Math.min(1,v)); Player.setVolumeLevel(volume);touch();};
B.shuffle = on => serial(async()=> {
  shuffle=on;
  if(on && index>=0) {for(let i=queue.length-1;i>index+1;i--) {const j=index+1+Math.floor(Math.random()*(i-index));[queue[i],queue[j]]=[queue[j],queue[i]];}}
  await prepare();touch();
});
B.repeat = mode => serial(async()=> {repeat=mode;await prepare();touch();});
B.enqueue = (ids,next) => serial(async()=> {const tracks=await metadata(ids);queue.splice(next ? Math.max(0,index+1):queue.length,0,...tracks);await prepare();touch();});
B.warm = ids => metadata(ids.slice(0,20)).catch(()=>{});
B.autoplay = () => {}; // Personal mixes are browsed explicitly; no invented radio feed.
B.playStation = () => fail(new Error('Tidal radio is not available'));
B.preview = id => {
  const saved = {queue:queue.slice(), index, pos:Player.getAssetPosition(),playing:Player.getPlaybackState()==='PLAYING',source:B.source};
  return B.playIds([id],id,B.source).then(()=> {preview=saved;touch();});
};
B.stopPreview = () => serial(async()=> {
  if(!preview)return; const saved=preview;preview=null;generation++; queue=saved.queue;index=saved.index;B.source=saved.source;
  if(index>=0) {await Player.load(product(index),saved.pos); if(saved.playing)await Player.play(); await prepare();}else await Player.reset();touch();
});
B.setToken = () => {B.needsAuth=false;B.err='';touch();};
B.audioOptions = (quality, normalization) => serial(async()=> {requestedQuality=quality;Player.setStreamingWifiAudioQuality(quality);Player.setLoudnessNormalizationMode(normalization);await prepare();touch();});
B.state = () => {
  const track=queue[index] || {}, ctx=Player.getPlaybackContext(), state=Player.getPlaybackState();
  const active=Player.getMediaProduct();
  const current=active?.referenceId===generation+':'+index;
  return JSON.stringify({rev:B.rev,ready:B.ready,err:B.err,needsAuth:B.needsAuth,playing:current && (state==='PLAYING'||state==='STALLED'),
    title:track.title||'',artist:track.artist||'',album:track.album||'',artwork:track.artwork||'',id:track.id||'',source:B.source,
    pos:current ? Player.getAssetPosition()||0:0,dur:current ? ctx?.actualDuration||track.duration||0:track.duration||0,index,length:queue.length,
    volume,shuffle,repeat,preview:preview ? {title:track.title,artist:track.artist,artwork:track.artwork}:null,
    requestedQuality, audioQuality:current ? ctx?.actualAudioQuality||'':'',codec:current ? ctx?.codec||'':'',sampleRate:current ? ctx?.sampleRate||0:0,
    bitDepth:current ? ctx?.bitDepth||0:0,presentation:current ? ctx?.actualAssetPresentation||'':''});
};
B.wait = (rev,ms) => {
  if (B.rev!==rev) return Promise.resolve(B.state());
  return new Promise(resolve=>{const finish=()=>{clearTimeout(timer);if(waiter===finish)waiter=null;resolve(B.state());};const timer=setTimeout(finish,ms);waiter=finish;});
};
const visualizers = new WeakMap();
function analyzer() {
  const media=Player.getMediaElement(); if(!media)return null;
  if(visualizers.has(media))return visualizers.get(media);
  try {const ctx=new AudioContext();const node=ctx.createMediaElementSource(media),a=ctx.createAnalyser();a.fftSize=2048;node.connect(a);a.connect(ctx.destination);const value={ctx,a};visualizers.set(media,value);return value;}catch{return null;}
}
B.spectrum = n => {const v=analyzer();if(!v)return [];if(v.ctx.state==='suspended')v.ctx.resume();const bytes=new Uint8Array(v.a.frequencyBinCount);v.a.getByteFrequencyData(bytes);return Array.from({length:n},(_,i)=>bytes[Math.floor(i*bytes.length/n)]);};
B.wave = n => {const v=analyzer();if(!v)return [];const bytes=new Uint8Array(v.a.fftSize);v.a.getByteTimeDomainData(bytes);return Array.from({length:n},(_,i)=>bytes[Math.floor(i*bytes.length/n)]-128);};
Player.events.addEventListener('media-product-transition',event=> {
  const reference=event.detail.mediaProduct.referenceId;
  const [gen,i]=String(reference).split(':').map(Number);
  if(gen!==generation || !queue[i] || queue[i].id !== event.detail.mediaProduct.productId)return;
  index=i;touch();
  try {requireFull();} catch(error) {fail(error);Player.pause().catch(fail);return;}
  serial(async()=> {
    const gen=generation;
    const followingIndex=following();
    if(followingIndex>=0) {const id=queue[followingIndex].id;await metadata([id]);if(gen!==generation)return;queue[followingIndex]=cache.get(id)||queue[followingIndex];}
    await prepare();touch();
  });
});
Player.events.addEventListener('playback-state-change',()=> {
  const v=visualizers.get(Player.getMediaElement());
  if(v) {if(Player.getPlaybackState()==='PLAYING')v.ctx.resume();else v.ctx.suspend();}
  touch();
});
for(const name of ['playback-quality-changed','ended'])Player.events.addEventListener(name,touch);
Player.events.addEventListener('error',event=> {if(!loading)fail(event.detail);});
Player.events.addEventListener('streaming-privileges-revoked',()=>fail(new Error('Tidal playback moved to another device')));
async function init() {
  Player.setCredentialsProvider(credentials);
  await Events.init({appInfo:{appName:'brumm',appVersion:CFG.version},blockedConsentCategories:{NECESSARY:false,PERFORMANCE:true,TARGETING:true},credentialsProvider:credentials,
    platform:{browserName:'Chrome',browserVersion:navigator.userAgent.match(/Chrome\/([\d.]+)/)?.[1]||'',osName:'Linux'},
    tlConsumerUri:'https://ec.tidal.com/api/event-batch',tlPublicConsumerUri:'https://ec.tidal.com/api/public/event-batch'});
  Player.setEventSender(Events);
  Player.bootstrap({outputDevices:false,players:[{player:'shaka',itemTypes:['track']},{player:'browser',itemTypes:['track']}]});
  Player.setAudioAdaptiveBitrateStreaming(false);Player.setTransitionMode(0);
  Player.setStreamingWifiAudioQuality(CFG.quality||'HIGH');Player.setLoudnessNormalizationMode(CFG.normalization||'TRACK');
  await credentials.getCredentials();B.ready=true;touch();
}
let initTimer;
Promise.race([init(),new Promise((_,reject)=>{initTimer=setTimeout(()=>reject(new Error('Tidal player initialization timed out')),30000);})]).finally(()=>clearTimeout(initTimer)).catch(fail);
