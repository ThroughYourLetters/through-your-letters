ALTER TABLE comments
    ADD COLUMN IF NOT EXISTS user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'VISIBLE',
    ADD COLUMN IF NOT EXISTS moderated_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS moderated_by TEXT,
    ADD COLUMN IF NOT EXISTS moderation_reason TEXT,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_comments_status'
    ) THEN
        ALTER TABLE comments
            ADD CONSTRAINT chk_comments_status CHECK (status IN ('VISIBLE', 'HIDDEN'));
    END IF;
END$$;

CREATE INDEX IF NOT EXISTS idx_comments_lettering_status_created
    ON comments(lettering_id, status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_comments_status_created
    ON comments(status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_comments_user_id
    ON comments(user_id);
