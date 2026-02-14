ALTER TABLE letterings
    ADD COLUMN IF NOT EXISTS moderation_reason TEXT,
    ADD COLUMN IF NOT EXISTS moderated_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS moderated_by TEXT;

CREATE TABLE IF NOT EXISTS lettering_status_history (
    id UUID PRIMARY KEY,
    lettering_id UUID NOT NULL REFERENCES letterings(id) ON DELETE CASCADE,
    from_status TEXT,
    to_status TEXT NOT NULL,
    reason TEXT,
    actor_type TEXT NOT NULL DEFAULT 'SYSTEM',
    actor_sub TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_lettering_status_history_actor_type
        CHECK (actor_type IN ('SYSTEM', 'USER', 'ADMIN', 'AUTO'))
);

CREATE INDEX IF NOT EXISTS idx_lettering_status_history_lettering_created
    ON lettering_status_history(lettering_id, created_at DESC);

CREATE TABLE IF NOT EXISTS lettering_metadata_history (
    id UUID PRIMARY KEY,
    lettering_id UUID NOT NULL REFERENCES letterings(id) ON DELETE CASCADE,
    edited_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    field_name TEXT NOT NULL,
    old_value TEXT,
    new_value TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_lettering_metadata_history_field_name
        CHECK (field_name IN ('description', 'contributor_tag', 'pin_code'))
);

CREATE INDEX IF NOT EXISTS idx_lettering_metadata_history_lettering_created
    ON lettering_metadata_history(lettering_id, created_at DESC);

CREATE OR REPLACE FUNCTION log_lettering_status_history()
RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        INSERT INTO lettering_status_history (
            id,
            lettering_id,
            from_status,
            to_status,
            reason,
            actor_type,
            actor_sub,
            created_at
        )
        VALUES (
            uuid_generate_v4(),
            NEW.id,
            NULL,
            NEW.status,
            COALESCE(NULLIF(NEW.moderation_reason, ''), 'Upload created'),
            CASE WHEN NEW.user_id IS NULL THEN 'SYSTEM' ELSE 'USER' END,
            NEW.user_id::TEXT,
            NEW.created_at
        );
    ELSIF TG_OP = 'UPDATE' AND NEW.status IS DISTINCT FROM OLD.status THEN
        INSERT INTO lettering_status_history (
            id,
            lettering_id,
            from_status,
            to_status,
            reason,
            actor_type,
            actor_sub,
            created_at
        )
        VALUES (
            uuid_generate_v4(),
            NEW.id,
            OLD.status,
            NEW.status,
            NULLIF(NEW.moderation_reason, ''),
            CASE
                WHEN NEW.moderated_by IS NOT NULL THEN 'ADMIN'
                WHEN NEW.user_id IS NOT NULL THEN 'USER'
                ELSE 'SYSTEM'
            END,
            COALESCE(NEW.moderated_by, NEW.user_id::TEXT),
            COALESCE(NEW.moderated_at, NOW())
        );
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_log_lettering_status_history ON letterings;

CREATE TRIGGER trg_log_lettering_status_history
AFTER INSERT OR UPDATE OF status, moderation_reason, moderated_by, moderated_at
ON letterings
FOR EACH ROW
EXECUTE FUNCTION log_lettering_status_history();

INSERT INTO lettering_status_history (
    id,
    lettering_id,
    from_status,
    to_status,
    reason,
    actor_type,
    actor_sub,
    created_at
)
SELECT
    uuid_generate_v4(),
    l.id,
    NULL,
    l.status,
    COALESCE(NULLIF(l.moderation_reason, ''), 'Initial imported state'),
    CASE WHEN l.user_id IS NULL THEN 'SYSTEM' ELSE 'USER' END,
    l.user_id::TEXT,
    l.created_at
FROM letterings l
WHERE NOT EXISTS (
    SELECT 1
    FROM lettering_status_history h
    WHERE h.lettering_id = l.id
);
