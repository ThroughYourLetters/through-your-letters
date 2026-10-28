-- Preserve historical coordinates for reviewed repair.
ALTER TABLE letterings
    ALTER COLUMN location DROP NOT NULL,
    ADD COLUMN location_source TEXT NOT NULL DEFAULT 'legacy_unverified',
    ADD COLUMN location_error TEXT;

ALTER TABLE letterings ADD CONSTRAINT letterings_location_source_check CHECK (
    (location_source = 'legacy_unverified' AND location_error IS NULL)
    OR (location_source = 'postal_code' AND location IS NOT NULL AND location_error IS NULL)
    OR (location_source = 'unresolved' AND location IS NULL AND location_error IS NOT NULL)
);
