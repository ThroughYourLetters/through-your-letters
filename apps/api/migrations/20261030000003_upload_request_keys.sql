CREATE TABLE upload_requests (
    id UUID PRIMARY KEY,
    owner_id UUID REFERENCES users(id) ON DELETE CASCADE,
    request_key UUID NOT NULL,
    payload_hash VARCHAR(64) NOT NULL CHECK (payload_hash ~ '^[0-9a-f]{64}$'),
    lettering_id UUID REFERENCES letterings(id) ON DELETE SET NULL,
    analysis_requested BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE NULLS NOT DISTINCT (owner_id, request_key)
);

CREATE INDEX upload_requests_lettering ON upload_requests(lettering_id)
WHERE lettering_id IS NOT NULL;
