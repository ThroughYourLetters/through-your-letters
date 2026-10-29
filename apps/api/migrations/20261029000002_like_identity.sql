-- Authenticated accounts can share an address. Anonymous votes still deduplicate
-- by address; neither identity can violate its own uniqueness rule.
ALTER TABLE likes DROP CONSTRAINT likes_lettering_id_user_ip_key;
CREATE UNIQUE INDEX idx_likes_anonymous_lettering
    ON likes(lettering_id, user_ip) WHERE user_id IS NULL;

-- The ALTER lock excludes competing like writes until this migration commits.
UPDATE letterings l
SET likes_count = counts.actual
FROM (
    SELECT l.id, COUNT(k.id)::int AS actual
    FROM letterings l LEFT JOIN likes k ON k.lettering_id = l.id
    GROUP BY l.id
) counts
WHERE l.id = counts.id AND l.likes_count IS DISTINCT FROM counts.actual;
