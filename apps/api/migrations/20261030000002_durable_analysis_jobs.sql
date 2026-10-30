CREATE TABLE analysis_jobs (
    lettering_id UUID PRIMARY KEY REFERENCES letterings(id) ON DELETE CASCADE,
    state TEXT NOT NULL DEFAULT 'ready' CHECK (state IN ('ready', 'running', 'completed', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts INTEGER NOT NULL CHECK (max_attempts BETWEEN 1 AND 100),
    available_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    lease_token UUID,
    lease_expires_at TIMESTAMPTZ,
    last_error_code TEXT CHECK (last_error_code IN ('processing_failed', 'timed_out', 'lease_expired')),
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (attempts <= max_attempts),
    CHECK (state = 'ready' OR attempts > 0),
    CHECK (state <> 'ready' OR attempts < max_attempts),
    CHECK ((state = 'running') = (lease_token IS NOT NULL AND lease_expires_at IS NOT NULL)),
    CHECK (state = 'running' OR (lease_token IS NULL AND lease_expires_at IS NULL)),
    CHECK ((state IN ('completed', 'failed')) = (finished_at IS NOT NULL))
);

CREATE INDEX analysis_jobs_ready ON analysis_jobs(available_at, lettering_id) WHERE state = 'ready';
CREATE INDEX analysis_jobs_expired ON analysis_jobs(lease_expires_at, lettering_id) WHERE state = 'running';

-- Legacy Redis jobs can be reconstructed from their unreviewed source uploads.
INSERT INTO analysis_jobs(lettering_id, max_attempts)
SELECT id, 3 FROM letterings WHERE status = 'PENDING' AND moderated_at IS NULL;
