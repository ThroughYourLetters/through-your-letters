CREATE TABLE IF NOT EXISTS location_revisits (
    id UUID PRIMARY KEY,
    original_lettering_id UUID NOT NULL REFERENCES letterings(id) ON DELETE CASCADE,
    revisit_lettering_id UUID NOT NULL REFERENCES letterings(id) ON DELETE CASCADE,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(original_lettering_id, revisit_lettering_id)
);

CREATE INDEX IF NOT EXISTS idx_location_revisits_original
    ON location_revisits(original_lettering_id);

CREATE INDEX IF NOT EXISTS idx_location_revisits_revisit
    ON location_revisits(revisit_lettering_id);
