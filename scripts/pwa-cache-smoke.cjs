// Run against a production build served by Vite preview, not the dev server.
const assert = require('node:assert/strict');
const {chromium}=require('playwright');
const origin=process.env.TTL_PREVIEW_URL || 'http://localhost:5175';
(async()=>{
 const browser=await chromium.launch({headless:true,channel:'chromium'});
 try {
  const context=await browser.newContext();
  const page=await context.newPage();
  await page.goto(origin+'/cache-policy.js');
  await page.evaluate(async()=>{
   const cache=await caches.open('api-cache');
   await cache.put('/api/v1/auth/me',new Response(JSON.stringify({email:'previous-user-secret'})));
  });
  await page.goto(origin);
  await page.evaluate(async()=>{
   await navigator.serviceWorker.ready;
   if (!navigator.serviceWorker.controller) await new Promise(resolve=>navigator.serviceWorker.addEventListener('controllerchange',resolve,{once:true}));
  });
  assert.equal(await page.evaluate(()=>caches.has('api-cache')),false,'Legacy private API cache was not deleted');
  await page.evaluate(async()=>{await fetch('/api/v1/auth/me').catch(()=>{});});
  const apiEntries=await page.evaluate(async()=>{
   const entries=[];for (const name of await caches.keys()) {
    for(const request of await (await caches.open(name)).keys()) {
     if(new URL(request.url).pathname.startsWith('/api/')) entries.push(request.url);
    }
   } return entries;
  });
  assert.deepEqual(apiEntries,[],'API response persisted in Cache Storage');
  await context.setOffline(true);
  const privateFetch=await page.evaluate(async()=>{try {await fetch('/api/v1/auth/me');return 'returned'}catch{return 'network failure'}});
  assert.equal(privateFetch,'network failure','Offline request returned old private data');
  console.log('PASS production service worker: legacy private-cache migration, no API caching, no private offline fallback');
  await context.close();
 }finally{await browser.close()}
})().catch(e=>{console.error(e);process.exitCode=1});
