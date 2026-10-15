import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';
const html = fs.readFileSync(new URL('../../crates/brumm/src/provider/spotify/player.html', import.meta.url), 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1].replace('__BRUMM_CFG__', '{version:"test"}');
const A='1111111111111111111111', B='2222222222222222222222';
function setup({firstFails=false}={}) {
 const requests=[], timers=[], credentials=[];
 const context=vm.createContext({window:{addEventListener(){}}, location:{pathname:'/secret'}, AbortSignal:{timeout(){}}, performance:{now:()=>0},
 setTimeout:(fn,ms)=>{timers.push({fn,ms});return timers.length;},clearTimeout(){},
 fetch:async (url,options)=>{
 if(url==='/secret/credentials') { credentials.push(JSON.parse(options.body));return {ok:true,status:200,json:async()=>({accessToken:'test'})}; }
 if(url==='/secret/tracks') { return {ok:true,status:200,json:async()=>JSON.parse(options.body).ids.map(id=>({id,title:'Title '+id}))}; }
 requests.push({url,body:options.body ? JSON.parse(options.body) : null}); const fail=firstFails && requests.length===1;
 return {status:fail?404:204,ok:!fail,json:async()=>({error:{message:'Device not found'}}),text:async()=>''};}});
 vm.runInContext(script,context);
 const player=vm.runInContext('B',context); player.device='test'; player.ready=true;
 return {context,player,requests,timers,credentials};
}
const flush=async()=>{for(let i=0;i<30;i++)await Promise.resolve();};
test('duplicate songs advance the queue by occurrence', async()=>{
 const {context,player}=setup();
 await player.playIds([A,A,B],A,'test',0);
 const state=(position,next,previous)=>({paused:false,position,duration:180000,track_window:{current_track:{id:A,name:'A'},next_tracks:next,previous_tracks:previous}});
 vm.runInContext('follow',context)(state(179000,[{id:A},{id:B}],[]));
 vm.runInContext('follow',context)(state(0,[{id:B}],[{id:A}]));
 assert.equal(player.q.pos,1);
});
test('a delayed device retry cannot supersede a newer play', async()=>{
 const {player,requests,timers}=setup({firstFails:true});
 const older=player.playIds([A],A,'older',0);
 await flush();
 assert(timers.some(t=>t.ms===1000));
 const newer=player.playIds([B],B,'newer',0);
 timers.find(t=>t.ms===1000).fn();
 await Promise.all([older,newer]);
 assert.deepEqual(requests.filter(r=>r.url.includes('/play?')).map(r=>r.body.uris[0]),['spotify:track:'+A,'spotify:track:'+B]);
 assert.equal(player.q.ids[0],B);
});
test('a station and a normal queue share command ordering',async()=>{
 const {player,requests,timers}=setup({firstFails:true});
 const older=player.playIds([A],A,'older',0); await flush();
 const newer=player.playStation('spotify:playlist:'+B,'station',A,12);
 timers.find(t=>t.ms===1000).fn(); await Promise.all([older,newer]);
 assert.equal(requests.findLast(r=>r.url.includes('/play?')).body.context_uri,'spotify:playlist:'+B);
 assert.equal(requests.findLast(r=>r.url.includes('/play?')).body.position_ms,12000);
});
test('queue metadata is fetched through the private bridge',async()=>{
 const {player,requests}=setup(); await player.playIds([A,B],A,'queue',0);
 const result=JSON.parse(await player.queue(2));
 assert.equal(result.items.length,2); assert.equal(result.items[1].title,'Title '+B);
 assert.equal(requests.filter(r=>r.url.includes('/play?')).length,1);
});
test('repeat-one keeps duplicate track on the same queue position',async()=>{
 const {context,player}=setup(); await player.playIds([A,A,B],A,'test',0); player.rep=1;
 const follow=vm.runInContext('follow',context);
 const state=p=>({paused:false,position:p,duration:180000,track_window:{current_track:{id:A},previous_tracks:[{id:A}],next_tracks:[{id:B}]}});
 follow(state(179000)); follow(state(0)); assert.equal(player.q.pos,0);
});

test('previous selects the preceding occurrence of a duplicate',async()=>{
 const {context,player}=setup(); await player.playIds([A,A,B],B,'test',0);
 player.q.win=[0,1,2]; player.player={previousTrack:async()=>{}};
 player.ps={paused:true,position:0}; player.prev();
 vm.runInContext('follow',context)({paused:false,position:0,duration:180000,track_window:{current_track:{id:A}}});
 assert.equal(player.q.pos,1);
});

test('switching from a playlist context resets remote shuffle and repeat',async()=>{
 const {player,requests}=setup(); player.rep=2; player.shuf=true;
 await player.playStation('spotify:playlist:'+B,'station','',0);
 assert(requests.some(r=>r.url.includes('repeat?state=context')));
 assert(requests.some(r=>r.url.includes('shuffle?state=true')));
 requests.length=0;
 await player.playIds([A,B],A,'queue',0);
 assert(requests.some(r=>r.url.includes('repeat?state=off')));
 assert(requests.some(r=>r.url.includes('shuffle?state=false')));
});
