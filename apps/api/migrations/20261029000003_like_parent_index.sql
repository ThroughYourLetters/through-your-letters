-- Keep an index over every vote for parent counts and cascading deletes. A
-- nullable user_id already permits multiple anonymous rows under PostgreSQL's
-- default NULLS DISTINCT uniqueness; the anonymous IP index handles their identity.
DROP INDEX idx_likes_user_lettering;
CREATE UNIQUE INDEX idx_likes_user_lettering ON likes(lettering_id, user_id);
