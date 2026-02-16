# API Reference

Base URL (dev): `http://localhost:3000`

## Health
### `GET /health`
Returns service and database status.

## Public Letterings
### `GET /api/v1/letterings`
Query params:
- `limit` (default `50`)
- `offset` (default `0`)
- `city_id` (optional UUID)
- `script` (optional)
- `style` (optional)
- `sort_by` (`newest` | `oldest` | `popular`)

Response:
```json
{
  "letterings": [],
  "total": 0,
  "limit": 50,
  "offset": 0
}
```

### `GET /api/v1/letterings/search?q=...`
Full-text and contributor search over approved records.

### `GET /api/v1/letterings/:id`
Returns one lettering. Includes `is_owner` when ownership can be resolved.

### `DELETE /api/v1/letterings/:id`
Owner-only deletion endpoint.
- Authenticated owner: matched by `user_id`.
- Legacy anonymous owner: IP fallback.

### `POST /api/v1/letterings/upload`
Multipart form fields:
- `image` (required)
- `contributor_tag` (required)
- `pin_code` (required)
- `city_id` (optional)
- `description` (optional)

Behavior:
- Virus scan check (if enabled)
- Dedupe by image hash
- Upload image + thumbnail to R2
- Queue ML processing (or auto-approve fallback)

### `POST /api/v1/letterings/:id/report`
Body:
```json
{ "reason": "..." }
```

### `GET /api/v1/letterings/:id/download`
Redirects to original image URL.

### `GET /api/v1/letterings/:id/similar`
Returns similar approved records.

### `GET /api/v1/letterings/:id/revisits`
### `POST /api/v1/letterings/:id/revisits`
Link and read before/after revisit relationships.

## Social
### `POST /api/v1/letterings/:id/like`
Toggle like by requester IP.

### `GET /api/v1/letterings/:id/comments`
### `POST /api/v1/letterings/:id/comments`
Comment constraints:
- non-empty
- max length 500
- rate-limited (per IP)

## Contributors
### `GET /api/v1/contributors/:tag`
Contributor uploads with pagination.

## Cities and Geo
### `GET /api/v1/cities`
### `GET /api/v1/cities/:id`
### `GET /api/v1/cities/:id/stats`
### `GET /api/v1/geo/markers`
### `GET /api/v1/geo/nearby`
### `GET /api/v1/geo/coverage`

## Community
### `GET /api/v1/community/leaderboard`
### `GET /api/v1/collections`
### `POST /api/v1/collections`
### `GET /api/v1/collections/:id`
### `POST /api/v1/collections/:collection_id/items/:lettering_id`
### `DELETE /api/v1/collections/:collection_id/items/:lettering_id`
### `GET /api/v1/challenges`

## User Authentication
### `POST /api/v1/auth/register`
Body:
```json
{
  "email": "user@example.com",
  "password": "min-8-chars",
  "display_name": "optional"
}
```

### `POST /api/v1/auth/login`
### `GET /api/v1/auth/me`
`/me` requires bearer user token.

## User Workspace (Bearer user token)
### `GET /api/v1/me/letterings`
Query: `limit`, `offset`, `status`.

### `PATCH /api/v1/me/letterings/:id`
Body fields (optional):
- `description`
- `contributor_tag`
- `pin_code`

### `GET /api/v1/me/notifications`
### `POST /api/v1/me/notifications/:id/read`

## Admin Authentication
### `POST /api/v1/admin/login`
Returns admin JWT.

## Admin Moderation (Bearer admin token)
### `GET /api/v1/admin/moderation`
### `POST /api/v1/admin/letterings/:id/approve`
### `POST /api/v1/admin/letterings/:id/reject`
### `DELETE /api/v1/admin/letterings/:id`
### `POST /api/v1/admin/letterings/:id/clear-reports`
### `GET /api/v1/admin/stats`

## WebSocket
### `GET /ws/feed`
Broadcasts processing events (e.g., `PROCESSED`).

## Error Contract
All errors use:
```json
{ "error": "message" }
```
Common statuses: `400`, `401`, `403`, `404`, `429`, `500`.
