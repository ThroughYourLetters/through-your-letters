# Environment Variables

This repository uses per-app environment files.

## Files
- Backend: `apps/api/.env`
- Frontend: `apps/web/.env`

Do not commit real secrets.

## Backend (`apps/api/.env`)

### Required
```bash
DATABASE_URL=postgresql://postgres:postgres@localhost:5432/through-your-letters
REDIS_URL=redis://localhost:6379

R2_ACCESS_KEY_ID=your_storage_access_key_id
R2_SECRET_ACCESS_KEY=your_storage_secret_access_key
R2_ENDPOINT=https://<account_id>.r2.cloudflarestorage.com
R2_REGION=auto
R2_FORCE_PATH_STYLE=false
R2_BUCKET_NAME=through-your-letters
R2_PUBLIC_URL=https://<public_r2_domain>

JWT_SECRET=replace_with_long_random_secret
ADMIN_EMAIL=admin@example.com
ADMIN_PASSWORD_HASH=$2b$12$...
```

### Optional (with defaults)
```bash
DATABASE_MAX_CONNECTIONS=20
HOST=0.0.0.0
PORT=3000

HUGGINGFACE_TOKEN=
ENABLE_ML_PROCESSING=true
ML_MODEL_PATH=./models/text_detector.onnx

ENABLE_VIRUS_SCAN=false
CLAMAV_HOST=clamav
CLAMAV_PORT=3310

RATE_LIMIT_UPLOADS_PER_IP=100

ENABLE_PENDING_AUTO_APPROVE=true
PENDING_AUTO_APPROVE_MINUTES=30
PENDING_AUTO_APPROVE_INTERVAL_SECONDS=300
PENDING_AUTO_APPROVE_BATCH_SIZE=50

IGNORE_MISSING_MIGRATIONS=true
RUST_LOG=info
```

### Generate `ADMIN_PASSWORD_HASH`
Use bcrypt (cost 12):
```bash
python - <<'PY'
import bcrypt
print(bcrypt.hashpw(b"change-me", bcrypt.gensalt(rounds=12)).decode())
PY
```

## Frontend (`apps/web/.env`)

### Required
```bash
VITE_API_URL=http://localhost:3000
```

## Validation Checklist
- API starts successfully with `cargo run`.
- `/health` returns `healthy`.
- Frontend can load cities and gallery.
- Upload succeeds and files appear in R2.


## Security Notes
- Use distinct secrets per environment.
- Rotate credentials on compromise.
- Never put secrets in client-side variables (`VITE_*`).
