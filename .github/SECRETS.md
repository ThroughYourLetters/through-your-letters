# GitHub Secrets - Free Services

## Required Secrets

### For Render Deployment (Backend)

**None required** - Render auto-deploys from GitHub

Optional:
```
RENDER_DEPLOY_HOOK=https://api.render.com/deploy/srv-xxx
```
Get from: Render Dashboard → Service → Settings → Deploy Hook

### For Vercel Deployment (Frontend)

```
VERCEL_TOKEN=xxx
VERCEL_ORG_ID=team_xxx
VERCEL_PROJECT_ID=prj_xxx
VITE_API_URL=https://through-your-letters-api.onrender.com
```

Get from:
- Token: https://vercel.com/account/tokens
- Org/Project IDs: Vercel Dashboard → Settings

### For Testing (CI)

```
R2_ACCESS_KEY_ID=test_key
R2_SECRET_ACCESS_KEY=test_secret
```

(Use dummy values for CI tests)

## Environment Variables in Services

### Render (Backend)

Set in: Dashboard → Service → Environment

```bash
DATABASE_URL=postgresql://...          # From Supabase
REDIS_URL=redis://...                  # From Upstash
R2_ACCESS_KEY_ID=...                   # From Cloudflare
R2_SECRET_ACCESS_KEY=...               # From Cloudflare
R2_ENDPOINT=...                        # From Cloudflare
R2_BUCKET_NAME=through-your-letters
R2_PUBLIC_URL=...                      # From Cloudflare
CORS_ALLOWED_ORIGINS=https://your-app.vercel.app
```

### Vercel (Frontend)

Set in: Project → Settings → Environment Variables

```bash
VITE_API_URL=https://through-your-letters-api.onrender.com
```

## Free Service URLs

- **Render**: https://dashboard.render.com
- **Vercel**: https://vercel.com/dashboard
- **Supabase**: https://app.supabase.com
- **Upstash**: https://console.upstash.com
- **Cloudflare**: https://dash.cloudflare.com

