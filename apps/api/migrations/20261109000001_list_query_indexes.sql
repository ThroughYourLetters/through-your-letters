-- Build indexes before replacing the visibility view so public reads can continue.
CREATE INDEX letterings_created_id ON letterings(created_at DESC, id DESC);
CREATE INDEX letterings_approved_popular
    ON letterings(likes_count DESC, created_at DESC, id DESC) WHERE status = 'APPROVED';
CREATE INDEX letterings_approved_city_created
    ON letterings(city_id, created_at DESC, id DESC) WHERE status = 'APPROVED';
CREATE INDEX letterings_status_created ON letterings(status, created_at, id);
CREATE INDEX letterings_owner_created ON letterings(user_id, created_at DESC, id DESC);
DROP INDEX idx_letterings_user_id;

CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX letterings_search_substrings ON letterings USING gin (
    detected_text gin_trgm_ops, description gin_trgm_ops, contributor_tag gin_trgm_ops
);
CREATE INDEX letterings_search_simple ON letterings USING gin (
    to_tsvector('simple'::regconfig, COALESCE(detected_text, '') || ' ' || COALESCE(description, ''))
);
