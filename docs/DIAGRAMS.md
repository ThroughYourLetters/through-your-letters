# System Diagrams

## Architecture Overview

```
                                         +---------------+
                                         |   Cloudflare  |
                                         |      R2       |
                                         | (Object Store)|
                                         +-------+-------+
                                                 ^
                                                 |
                      +-----------+     upload   |
    Internet  +-----> |  Frontend | ---------> +--+--+   +--------+
      Users   |       |  (React)  |            |API  |   | Workers|
              |       +-----------+            |(Rust|   |(ML/Job)|
              |                                  |)   |   +----+---+
              |                                  +--+--+        |
              |                                     |          |
              |                                     |          |
              |                              +------+------+   |
              |                              |  Redis      |   |
              |                              |  (Queue)    |   |
              |                              +------+------+   |
              |                                     |          |
              |                    Postgres/PostGIS (Data) <---+
              |                                     ^
              +-------------------------------------+

Components:
- Frontend (React/Vite) on public internet
- API (Rust/Axum) receives requests and enqueues jobs
- Workers process jobs from Redis and update Postgres
- R2 stores images; metadata stays in database
- WebSocket feed broadcasts processing events
```

## Database Schema (Core Tables)

```
┌─────────────────────────────────────────────────┐
│                    USERS                        │
├─────────────────────────────────────────────────┤
│ id (UUID, pk) | email | password_hash          │
│ display_name | role ('USER'|'ADMIN')            │
└──────────────────┬──────────────────────────────┘
                   │ 1:N (owns uploads)
                   │
        ┌──────────┴──────────────────────┐
        │                                 │
┌───────▼──────────────────────┐  ┌──────▼──────────────────────┐
│      LETTERINGS              │  │   NOTIFICATIONS             │
├──────────────────────────────┤  ├─────────────────────────────┤
│ id | user_id (fk)            │  │ id | user_id (fk)           │
│ city_id (fk) | image_url     │  │ type | title | body         │
│ status                       │  │ is_read | created_at        │
│ location (Geography)         │  └─────────────────────────────┘
│ ml_style | ml_script         │
│ likes_count | comments_count │
└───────┬──────────────────────┘
        │ 1:N
        │
        ├──────────────────┬──────────────────┬──────────────────┐
        │                  │                  │                  │
    ┌───▼────┐  ┌─────────▼──────┐  ┌────────▼────┐  ┌─────────▼─────┐
    │ COMMENTS│  │ LIKES          │  │ COLLECTIONS │  │ LOCATION_     │
    ├────────┤  ├────────────────┤  ├─────────────┤  │ REVISITS      │
    │ id     │  │ id | user_ip   │  │ id | creator│  │ original_id   │
    │ user_id│  │ created_at     │  │ is_public   │  │ revisit_id    │
    │ content│  └────────────────┘  └─────────────┘  └───────────────┘
    │ status │
    │ mod.*  │

┌──────────────────────────┐
│       CITIES             │
├──────────────────────────┤
│ id | name | country_code │
│ center_lat/lng | zoom    │
│ description               │
└──────────────────────────┘

┌──────────────────────────────────────┐
│     REGION_POLICIES                  │
├──────────────────────────────────────┤
│ country_code (pk)                    │
│ uploads_enabled                      │
│ comments_enabled                     │
│ discoverability_enabled              │
│ auto_moderation_level                │
└──────────────────────────────────────┘

Audit & History:
- lettering_status_history (tracks all status changes)
- lettering_metadata_history (tracks edits by users)
- admin_audit_logs (tracks admin actions)
```

## Supabase Deployment

When using Supabase (OCI or managed):

```
┌────────────────────────────────────────────┐
│         SUPABASE PROJECT                   │
├────────────────────────────────────────────┤
│                                            │
│  Auth Module (JWT generation)              │
│  └─> ttl_user_token (in cookie)            │
│  └─> ttl_admin_token (separate namespace)  │
│                                            │
│  PostgreSQL Database (with PostGIS)        │
│  └─> All tables above                      │
│  └─> Row-Level Security (RLS) policies     │
│  └─> Full-text search indexes              │
│                                            │
│  Connection Pooling (PgBouncer)            │
│  └─> Prevent connection pool exhaustion    │
│                                            │
└────────────────────────────────────────────┘
        ↓ (SQL queries)
┌────────────────────────────────────────────┐
│    Your Application (API + Workers)        │
├────────────────────────────────────────────┤
│ Reads JWT from Auth module                 │
│ Respects RLS policies automatically        │
│ Enqueues jobs to Redis                     │
└────────────────────────────────────────────┘
```
