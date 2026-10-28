// Requires the same Playwright tooling as scripts/redesign-smoke.cjs.
const assert = require('node:assert/strict');
const {chromium} = require('playwright');
const origin = process.env.TTL_PREVIEW_URL || 'http://localhost:5173';
const image='data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" width="300" height="240"><rect width="300" height="240" fill="%23cc543a"/></svg>';
const item = n => ({id:String(n),detected_text:`Sign ${n}`,image_url:image,thumbnail_urls:{small:image},pin_code:'560001',contributor_tag:'tester',status:'APPROVED',created_at:'2026-03-01',ml_metadata:{script:'Latin'}});
(async()=>{
 const browser = await chromium.launch({headless:true,channel:'chromium'});
 try {
  for (const width of [320,390,640,1024,1440]) {
   const context=await browser.newContext({viewport:{width,height:844},reducedMotion:'reduce',serviceWorkers:'block'});
   const searches=[];
   await context.route('https://fonts.googleapis.com/**',r=>r.abort());
   await context.routeWebSocket('**/ws/feed',()=>{});
   await context.route('**/api/v1/**',async route=>{
    const u=new URL(route.request().url());
    let data=[];
    if (u.pathname==='/api/v1/letterings') data={letterings:Array.from({length:12},(_,i)=>item(i+1)),total:12};
    if (u.pathname.endsWith('/search')) {
     const q=u.searchParams.get('q');searches.push(q);
     if (q==='slow') await new Promise(r=>setTimeout(r,1200));
     data=[item(q==='slow'?90:99)];
    }
    await route.fulfill({json:data}).catch(()=>{});
   });
   const page=await context.newPage();page.setDefaultTimeout(10000);
   await page.goto(origin);
   await page.getByRole('button',{name:'Open Sign 1',exact:true}).waitFor();
   const nav=page.getByRole('navigation',{name:'Quick navigation'});
   if (width<768) {
    const rect=await nav.boundingBox();
    assert.ok(rect.y+rect.height <=844 && rect.y+rect.height>=800);
    await page.evaluate(()=>window.scrollTo(0,document.documentElement.scrollHeight));
    const scrolled=await nav.boundingBox();assert.ok(Math.abs(scrolled.y-rect.y)<1,'Navigation moved while scrolling');
    await page.locator('#root').evaluate(e=>{e.style.transform='translateZ(0)'});
    const transformed=await nav.boundingBox();assert.ok(Math.abs(transformed.y-rect.y)<1,'Ancestor transform moved navigation');
    await page.locator('#root').evaluate(e=>{e.style.transform=''});
    const footer=await page.locator('footer').boundingBox();
    assert.ok(footer.y+footer.height<=844+1,'Footer could not scroll into viewport');
    await page.screenshot({path:`/tmp/ttl-navigation-${width}.png`});
   } else assert.equal(await nav.isVisible(),false);
   const search=page.getByRole('textbox',{name:'Search tags, scripts, neighborhoods...'});
   await search.fill('shop');
   await page.getByRole('heading',{name:'Search results',exact:true}).waitFor();
   await page.waitForTimeout(1600);
   assert.equal(searches.filter(x=>x==='shop').length,1,'Unchanged search repeated');
   await search.fill('slow');await page.waitForTimeout(500);
   await search.fill('fast');await page.getByRole('button',{name:'Open Sign 99',exact:true}).waitFor();
   await page.waitForTimeout(1200);
   assert.equal(await page.getByRole('button',{name:'Open Sign 90',exact:true}).count(),0,'Old search overwrote current results');
   await search.fill('slow');await page.waitForTimeout(500);
   await page.getByRole('button',{name:'Clear search'}).click();
   await page.waitForTimeout(1200);
   assert.equal(await page.getByRole('heading',{name:'Search results',exact:true}).count(),0,'Cleared search reappeared');
   console.log(`PASS ${width}px: viewport navigation, footer spacing, one request per search, stale-response cancellation`);
   await context.close();
  }
 } finally {await browser.close()}
})().catch(e=>{console.error(e);process.exitCode=1});
