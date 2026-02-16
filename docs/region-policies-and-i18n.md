# Region Policies and Locale-Aware Search

## What was added
- Region policy controls per country code (`region_policies` table).
- Admin APIs to list and update region policies.
- Enforcement in runtime:
  - Uploads blocked when `uploads_enabled = false`.
  - Comment creation blocked when `comments_enabled = false`.
  - Public gallery/search/geo results hidden when `discoverability_enabled = false`.
- Locale-aware search support via `lang` query parameter.
- Frontend language selector (`EN`, `HI`) with persisted locale.
- Frontend search now sends locale to backend search.
- Admin UI tab for region policy moderation.

## Database
Migration:
- `apps/api/migrations/20260221000001_add_region_policies.sql`

Schema:
- `country_code` (`VARCHAR(2)`, PK)
- `uploads_enabled` (`BOOLEAN`)
- `comments_enabled` (`BOOLEAN`)
- `discoverability_enabled` (`BOOLEAN`)
- `auto_moderation_level` (`relaxed|standard|strict`)
- timestamps

## Admin API
Protected by admin JWT middleware.

- `GET /api/v1/admin/region-policies`
  - Query:
    - `country_code` (optional)
    - `limit`, `offset`
  - Returns paginated list.

- `PUT /api/v1/admin/region-policies/{country_code}`
  - Body fields are optional:
    - `uploads_enabled`
    - `comments_enabled`
    - `discoverability_enabled`
    - `auto_moderation_level`
  - Upserts policy for the country.

## Public API change
- `GET /api/v1/letterings/search?q=...&lang=...`
  - `lang` is optional.
  - `en*` locales use `english` text config.
  - Other locales use `simple` text config fallback.

## Frontend
- Locale store: `apps/web/src/store/useLocaleStore.ts`
- Header language selector: `apps/web/src/components/Header.tsx`
- Locale-aware search call: `apps/web/src/components/SearchBar.tsx`
- Admin region controls UI: `apps/web/src/components/admin/AdminRegionPoliciesPanel.tsx`
