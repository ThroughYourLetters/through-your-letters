CREATE TABLE geocoding_cache (
    cache_key TEXT PRIMARY KEY CHECK (length(cache_key) = 64),
    response JSONB NOT NULL CHECK (jsonb_typeof(response) = 'array'),
    expires_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX geocoding_cache_expiry ON geocoding_cache (expires_at);
CREATE INDEX geocoding_cache_age ON geocoding_cache (updated_at, cache_key);

-- One gate protects every provider request.
CREATE TABLE geocoding_gate (
    id BOOLEAN PRIMARY KEY DEFAULT true CHECK (id),
    lease_id UUID,
    next_allowed_at TIMESTAMPTZ NOT NULL DEFAULT '-infinity'
);

INSERT INTO geocoding_gate (id) VALUES (true);
