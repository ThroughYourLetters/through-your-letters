-- Keep evidence after an upload disappears.
CREATE TABLE location_repairs (
    id UUID PRIMARY KEY,
    lettering_id UUID NOT NULL,
    reviewed_by TEXT NOT NULL,
    before_location JSONB NOT NULL,
    after_location JSONB NOT NULL,
    evidence JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX location_repairs_lettering_history ON location_repairs (lettering_id, created_at, id);
CREATE INDEX letterings_unresolved_location_report ON letterings (id)
    WHERE location_source IN ('legacy_unverified', 'unresolved');
