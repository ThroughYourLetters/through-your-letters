ALTER TABLE letterings
    ADD COLUMN location_name TEXT,
    ADD COLUMN location_accuracy_m DOUBLE PRECISION;

ALTER TABLE letterings DROP CONSTRAINT letterings_location_source_check;
ALTER TABLE letterings ADD CONSTRAINT letterings_location_source_check CHECK (
    (location_source = 'legacy_unverified' AND location_error IS NULL)
    OR (location_source = 'postal_code' AND location IS NOT NULL AND location_error IS NULL)
    OR (location_source = 'contributor_pin' AND location IS NOT NULL AND location_error IS NULL AND location_name IS NOT NULL)
    OR (location_source = 'unresolved' AND location IS NULL AND location_error IS NOT NULL)
);
ALTER TABLE letterings ADD CONSTRAINT letterings_location_metadata_check CHECK (
    (location_source = 'contributor_pin' OR (location_name IS NULL AND location_accuracy_m IS NULL))
    AND (location_name IS NULL OR char_length(btrim(location_name)) BETWEEN 1 AND 200)
    AND (location_accuracy_m IS NULL OR location_accuracy_m BETWEEN 1 AND 1000000)
);

ALTER TABLE lettering_metadata_history DROP CONSTRAINT chk_lettering_metadata_history_field_name;
ALTER TABLE lettering_metadata_history ADD CONSTRAINT chk_lettering_metadata_history_field_name
    CHECK (field_name IN ('description', 'contributor_tag', 'pin_code', 'location'));

-- PostgreSQL fixes SELECT * columns when a view is created.
CREATE OR REPLACE VIEW discoverable_letterings AS
SELECT l.*
FROM letterings l
JOIN cities c ON c.id = l.city_id
LEFT JOIN region_policies rp ON rp.country_code = c.country_code
WHERE l.status = 'APPROVED'
  AND COALESCE(rp.discoverability_enabled, true);
