-- Add user_id to likes for authenticated per-user deduplication.
-- Rows created before this migration remain ip-only (user_id NULL).
-- Partial unique index enforces one like per authenticated user per lettering.
ALTER TABLE likes
    ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE CASCADE;

CREATE UNIQUE INDEX IF NOT EXISTS idx_likes_user_lettering
    ON likes(lettering_id, user_id)
    WHERE user_id IS NOT NULL;
