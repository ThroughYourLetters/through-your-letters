ALTER TABLE cities DROP CONSTRAINT IF EXISTS cities_name_key;

CREATE UNIQUE INDEX IF NOT EXISTS idx_cities_name_country_unique
    ON cities(name, country_code);
