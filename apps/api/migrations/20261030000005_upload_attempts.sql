CREATE TABLE upload_attempts (
    id UUID PRIMARY KEY,
    lease_token UUID NOT NULL,
    state TEXT NOT NULL DEFAULT 'uploading' CHECK (state IN ('uploading','committed','cleanup')),
    lease_expires_at TIMESTAMPTZ NOT NULL,
    image_url TEXT NOT NULL CHECK (octet_length(image_url) BETWEEN 1 AND 4096),
    thumbnail_url TEXT NOT NULL CHECK (octet_length(thumbnail_url) BETWEEN 1 AND 4096),
    lettering_id UUID REFERENCES letterings(id) ON DELETE SET NULL,
    cleanup_rounds INTEGER NOT NULL DEFAULT 0 CHECK (cleanup_rounds BETWEEN 0 AND 100),
    next_cleanup_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK (state='committed' OR lettering_id IS NULL),
    CHECK ((state='cleanup')=(next_cleanup_at IS NOT NULL))
);

CREATE INDEX upload_attempts_expired ON upload_attempts(lease_expires_at,id) WHERE state='uploading';
CREATE INDEX upload_attempts_cleanup_due ON upload_attempts(next_cleanup_at,id) WHERE state='cleanup';
