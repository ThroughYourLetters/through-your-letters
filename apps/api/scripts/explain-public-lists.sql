\set ON_ERROR_STOP on
BEGIN READ ONLY;
SET LOCAL statement_timeout = '30s';
SET LOCAL lock_timeout = '1s';
\if :{?search}
\else
  \set search 'Lantern Works'
\endif
\if :{?city_id}
\else
  SELECT city_id FROM discoverable_letterings
  GROUP BY city_id ORDER BY COUNT(*) DESC, city_id LIMIT 1 \gset
\endif
\if :{?collection_id}
\else
  SELECT id AS collection_id FROM collections WHERE is_public
  ORDER BY created_at DESC, id DESC LIMIT 1 \gset
\endif

\echo 'Gallery count'
EXPLAIN (ANALYZE, BUFFERS, SETTINGS)
SELECT COUNT(*) FROM discoverable_letterings;

\echo 'Newest gallery page'
EXPLAIN (ANALYZE, BUFFERS, SETTINGS)
SELECT id, city_id, image_url, thumbnail_small, detected_text, description,
       ml_style, ml_script, likes_count, comments_count, created_at
FROM discoverable_letterings
ORDER BY created_at DESC, id DESC LIMIT 31;

\echo 'City gallery page'
EXPLAIN (ANALYZE, BUFFERS, SETTINGS)
SELECT id, city_id, image_url, thumbnail_small, detected_text, description,
       ml_style, ml_script, likes_count, comments_count, created_at
FROM discoverable_letterings WHERE city_id = :'city_id'::uuid
ORDER BY created_at DESC, id DESC LIMIT 31;

\echo 'Popular gallery page'
EXPLAIN (ANALYZE, BUFFERS, SETTINGS)
SELECT id, city_id, image_url, thumbnail_small, detected_text, description,
       ml_style, ml_script, likes_count, comments_count, created_at
FROM discoverable_letterings
ORDER BY likes_count DESC, created_at DESC, id DESC LIMIT 31;

\echo 'English text and substring search'
EXPLAIN (ANALYZE, BUFFERS, SETTINGS)
SELECT id, image_url, detected_text, description, likes_count, created_at
FROM discoverable_letterings
WHERE detected_text_tsv @@ websearch_to_tsquery('english', :'search')
   OR detected_text ILIKE '%' || :'search' || '%'
   OR description ILIKE '%' || :'search' || '%'
   OR contributor_tag ILIKE '%' || :'search' || '%'
ORDER BY likes_count DESC, created_at DESC, id DESC LIMIT 50;

\echo 'Unstemmed multilingual text and substring search'
EXPLAIN (ANALYZE, BUFFERS, SETTINGS)
SELECT id, image_url, detected_text, description, likes_count, created_at
FROM discoverable_letterings
WHERE to_tsvector('simple'::regconfig, COALESCE(detected_text, '') || ' ' || COALESCE(description, ''))
          @@ websearch_to_tsquery('simple', :'search')
   OR detected_text ILIKE '%' || :'search' || '%'
   OR description ILIKE '%' || :'search' || '%'
   OR contributor_tag ILIKE '%' || :'search' || '%'
ORDER BY likes_count DESC, created_at DESC, id DESC LIMIT 50;

\echo 'Postal coverage'
EXPLAIN (ANALYZE, BUFFERS, SETTINGS)
SELECT g.pin_code, g.city_id, c.name, g.lat, g.lng, g.count
FROM (
    SELECT pin_code, city_id, AVG(ST_Y(location::geometry)) AS lat,
           AVG(ST_X(location::geometry)) AS lng, COUNT(*) AS count
    FROM discoverable_letterings WHERE location IS NOT NULL
    GROUP BY pin_code, city_id
    ORDER BY COUNT(*) DESC, pin_code, city_id LIMIT 5000
) g JOIN cities c ON c.id = g.city_id
ORDER BY g.count DESC, g.pin_code, g.city_id;

\echo 'Public collection page and visible item counts'
EXPLAIN (ANALYZE, BUFFERS, SETTINGS)
SELECT c.id, c.name, c.description, c.creator_tag, c.is_public, c.created_at, COUNT(l.id)
FROM (
    SELECT id, name, description, creator_tag, is_public, created_at
    FROM collections WHERE is_public ORDER BY created_at DESC, id DESC LIMIT 50
) c
LEFT JOIN collection_items ci ON ci.collection_id = c.id
LEFT JOIN discoverable_letterings l ON l.id = ci.lettering_id
GROUP BY c.id, c.name, c.description, c.creator_tag, c.is_public, c.created_at
ORDER BY c.created_at DESC, c.id DESC;

\echo 'Visible collection items'
EXPLAIN (ANALYZE, BUFFERS, SETTINGS)
SELECT l.id, l.image_url, l.thumbnail_small, l.detected_text, l.contributor_tag
FROM collection_items ci JOIN discoverable_letterings l ON l.id = ci.lettering_id
WHERE ci.collection_id = :'collection_id'::uuid
ORDER BY l.created_at DESC, l.id DESC LIMIT 100;

\echo 'Pending moderation queue'
EXPLAIN (ANALYZE, BUFFERS, SETTINGS)
SELECT id, image_url, thumbnail_small, contributor_tag, pin_code, detected_text,
       description, status, likes_count, comments_count, report_count, report_reasons, created_at
FROM letterings WHERE status = 'PENDING'
ORDER BY created_at, id LIMIT 100;

ROLLBACK;
