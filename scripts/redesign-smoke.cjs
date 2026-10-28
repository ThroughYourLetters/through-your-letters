// Run against Vite with VITE_API_URL=same-origin. Requires Playwright in the
// tooling environment; no requests or mutations reach the production API.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const { chromium } = require('playwright');
const baseURL = process.env.TTL_PREVIEW_URL || 'http://localhost:5173';
const artifactDir = process.env.TTL_ARTIFACT_DIR || '/tmp/ttl-redesign-artifacts';
const image = 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="600" height="450"><rect width="600" height="450" fill="#cc543a"/><text x="40" y="240" font-size="65" fill="white">Bengaluru Letters</text></svg>');
const item = (id) => ({id: String(id), image_url: image, thumbnail_urls: {small: image, medium: image, large: image}, location: {type: 'Point', coordinates: [77.59,12.97]}, pin_code: '560001', contributor_tag: 'tester', detected_text: `Street sign ${id}`, description: 'A hand-painted shop sign.', status: 'APPROVED', created_at: '2026-03-01T00:00:00Z', likes_count: 2, comments_count: 0, ml_metadata: {style: 'Handpainted', script: 'Latin'}, is_owner: true});
const user = {id:'user-1',email:'tester@example.com',display_name:'Tester',role:'user',created_at:'2026-03-01T00:00:00Z'};
async function setup(context) {
  let deleted = false;
  const requests = [];
  await context.routeWebSocket('**/ws/feed', () => {});
  await context.route('**/api/v1/**', async route => {
    const req = route.request(), url = new URL(req.url()), path = url.pathname;
    requests.push({path, method:req.method(), query:url.search});
    let data = [];
    if (path === '/api/v1/cities') data = [{id:'city-1',name:'Bengaluru',country_code:'IN',center_lat:12.97,center_lng:77.59,default_zoom:11,is_active:true}];
    else if (path === '/api/v1/letterings') {
      const offset = Number(url.searchParams.get('offset') || 0);
      const all = Array.from({length:31}, (_,i)=>item(i+1)).filter(i => !deleted || i.id !== '1');
      data = {letterings:all.slice(offset,offset+30),total:all.length};
    } else if (path.endsWith('/comments')) {
      if (path.includes('/2/')) return route.fulfill({status:500,json:{error:'Unavailable'}});
      data = req.method() === 'POST' ? {id:'comment-1',content:JSON.parse(req.postData()).content,commenter_name:'Tester',created_at:'2026-03-01T00:00:00Z'} : [];
    } else if (path.endsWith('/revisits')) data = {revisits:[]};
    else if (path.endsWith('/similar')) data = {similar:[]};
    else if (path.endsWith('/like')) data = {liked:true,likes_count:3};
    else if (/\/letterings\/\d+$/.test(path)) {
      if (req.method() === 'DELETE') {deleted=true;data={};} else data=item(path.split('/').pop());
    } else if (path === '/api/v1/letterings/upload') data={id:'upload-test',status:'PENDING'};
    else if (path === '/api/v1/admin/login') data={token:'mock-admin'};
    else if (path === '/api/v1/admin/moderation') data={items:[],total:0};
    else if (path === '/api/v1/admin/stats') data={total_uploads:31,pending_approvals:0,approved:31,rejected:0,total_cities:1,total_likes:62,total_comments:0};
    else if (path.includes('/auth/login')) data = {token:'test-token',user};
    else if (path.includes('/auth/me')) data=user;
    else if (path.includes('/users/me/uploads')) data={items:[],total:0};
    else if (path.includes('/notifications')) data={items:[],total:0,unread_count:0};
    else if (path === '/api/v1/collections') data={items:[],total:0};
    else if (path === '/api/v1/collections/test') data={id:'test',name:'Test collection',creator_tag:'tester',items:[],total_items:0,created_at:'2026-03-01'};
    else if (path === '/api/v1/letterings/search') data=[item(1)];
    await route.fulfill({status:200,json:data});
  });
  return requests;
}
(async () => {
  await fs.mkdir(artifactDir,{recursive:true});
  const browser = await chromium.launch({headless:true, channel:"chromium"});
  try {
    for (const viewport of [{width:1440,height:1000},{width:390,height:844},{width:320,height:720}]) {
      const context = await browser.newContext({viewport,reducedMotion:'reduce',serviceWorkers:'block'});
      await context.route('https://fonts.googleapis.com/**', route => route.abort());
      const requests = await setup(context);
      const page = await context.newPage(), errors=[];
      page.setDefaultTimeout(10000);
      console.log(`Checking ${viewport.width}px`);
      page.on('pageerror', e=>errors.push(e.message));
      await page.goto(baseURL);
      await page.getByRole('heading',{name:'The Gallery',exact:true}).waitFor();
      await page.getByRole('button',{name:'Open Street sign 1',exact:true}).waitFor();
      assert.equal(await page.getByRole('link',{name:'Through Your Letters home'}).count(),1);
      const initialWord=await page.getByTitle('Click for another word',{exact:true}).getAttribute('aria-label');
      await page.waitForTimeout(2400);
      assert.equal(await page.getByTitle('Click for another word',{exact:true}).getAttribute('aria-label'), initialWord);
      assert.equal(await page.locator('html').evaluate(el=>getComputedStyle(el).scrollBehavior),'auto');
      await page.screenshot({path:`${artifactDir}/gallery-${viewport.width}.png`,fullPage:true});
      const city = page.getByRole('button',{name:'All Cities',exact:true});
      await city.click();
      await page.getByRole('button',{name:/Bengaluru.*IN/}).click();
      assert.equal(await page.getByRole('button',{name:'Bengaluru',exact:true}).evaluate(el=>el===document.activeElement),true);
      await page.getByRole('button',{name:'Bengaluru',exact:true}).click();
      await page.keyboard.press('Escape');
      assert.equal(await page.getByRole('dialog',{name:'Choose a city'}).count(),0);
      console.log("city picker passed");
      await page.getByRole('button',{name:'Latin',exact:true}).click();
      await page.waitForURL('**/*script=Latin*');
      await page.getByRole('button',{name:'Clear all filters'}).filter({visible:true}).click();
      console.log('filters passed');
      await page.getByRole('button',{name:'Open Street sign 1',exact:true}).click();
      await page.getByRole('heading',{name:'Street sign 1',exact:true}).waitFor();
      await page.getByRole('button',{name:'2 likes',exact:true}).click();
      await page.getByRole('button',{name:'3 likes',exact:true}).waitFor();
      await page.getByRole('button',{name:'View full size',exact:true}).click();
      await page.keyboard.press('Escape');
      await page.screenshot({path:`${artifactDir}/detail-${viewport.width}.png`,fullPage:true});
      await page.goto(`${baseURL}/lettering/2`);
      await page.getByText('Failed to load comments',{exact:true}).waitFor();
      for (const [path,heading] of [['/map','The Archive Heatmap'],['/community','Community'],['/contribute','Contributor Lab'],['/auth','Sign In'],['/collections/test','Test collection']]) {
        await page.goto(baseURL+path);
        await page.getByRole('heading',{name:heading,exact:true}).waitFor();
        assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth <= window.innerWidth+1),`Horizontal overflow at ${viewport.width}: ${path}`);
      }
      await page.goto(`${baseURL}/admin`);
      await page.getByRole('heading',{name:'Admin Portal'}).waitFor();
      await page.getByPlaceholder('Email',{exact:true}).fill('admin@example.com');
      await page.getByPlaceholder('Password',{exact:true}).fill('mock-password');
      await page.getByRole('button',{name:'Initialize Node'}).click();
      await page.getByRole('heading',{name:'Curator Control'}).waitFor();
      await page.goto(`${baseURL}/auth`);
      await page.getByPlaceholder('Email',{exact:true}).fill('tester@example.com');
      await page.locator('input[type="password"]').fill('test-password');
      await page.locator('form').getByRole('button',{name:'Sign In',exact:true}).click();
      await page.waitForURL('**/me/uploads');
      await page.goto(`${baseURL}/lettering/1`);
      await page.getByRole('textbox',{name:'Add a note'}).fill('A lovely discovery');
      await page.getByRole('button',{name:'Post comment'}).click();
      await page.getByText('A lovely discovery',{exact:true}).waitFor();
      await page.goto(baseURL);
      await page.getByRole('button',{name:'Open Street sign 1',exact:true}).waitFor();
      await page.locator('[class="flex justify-center py-6"]').scrollIntoViewIfNeeded();
      await page.getByRole('button',{name:'Open Street sign 31',exact:true}).waitFor();
      assert.ok(requests.some(r=>r.query.includes('offset=30')),'Pagination request missing');
      page.once('dialog', d=>d.accept());
      await page.getByRole('button',{name:'Delete upload',exact:true}).first().click();
      await page.getByText('Upload deleted',{exact:true}).waitFor();
      assert.equal(await page.getByRole('button',{name:'Open Street sign 1',exact:true}).count(),0);

      await page.getByRole('textbox',{name:'Search tags, scripts, neighborhoods...'}).fill('shop');
      await page.getByRole('heading',{name:'Search results',exact:true}).waitFor();
      await page.getByRole('button',{name:'Clear search'}).click();
      await page.getByRole('button',{name:'All Cities',exact:true}).click();
      await page.getByRole('button',{name:/Bengaluru.*IN/}).click();
      await page.getByRole('button',{name:'Add Discovery',exact:true}).first().click();
      await page.locator('input[type="file"]').first().setInputFiles({name:'test.png',mimeType:'image/png',buffer:Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jS1kAAAAASUVORK5CYII=','base64')});
      await page.getByPlaceholder('Contributor Name').fill('Tester');
      await page.getByPlaceholder('e.g. 560001, SW1A 1AA, 10001').fill('560001');
      await page.getByRole('button',{name:'Finalize Archiving'}).click();
      await page.getByText('1 artifact submitted',{exact:true}).waitFor();
      assert.ok(requests.some(r=>r.path==='/api/v1/letterings/upload' && r.method==='POST'));
      assert.deepEqual(errors,[],`Browser errors at ${viewport.width}`);
      console.log(`PASS ${viewport.width}px: gallery, filters, city keyboard, detail, lightbox, routes, login, admin, comments, deletion, pagination, search, upload, reduced motion`);
      await context.close();
    }
  } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
