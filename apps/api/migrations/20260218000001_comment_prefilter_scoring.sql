ALTER TABLE comments
    ADD COLUMN IF NOT EXISTS moderation_score INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS moderation_flags JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS auto_flagged BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS needs_review BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS review_priority INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_comments_needs_review_priority
    ON comments(needs_review, review_priority DESC, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_comments_auto_flagged
    ON comments(auto_flagged, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_comments_moderation_score
    ON comments(moderation_score DESC, created_at DESC);
