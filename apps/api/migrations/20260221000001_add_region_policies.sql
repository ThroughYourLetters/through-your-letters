CREATE TABLE IF NOT EXISTS region_policies (
    country_code VARCHAR(2) PRIMARY KEY,
    uploads_enabled BOOLEAN NOT NULL DEFAULT true,
    comments_enabled BOOLEAN NOT NULL DEFAULT true,
    discoverability_enabled BOOLEAN NOT NULL DEFAULT true,
    auto_moderation_level TEXT NOT NULL DEFAULT 'standard',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_region_policy_country_code_upper
        CHECK (country_code = UPPER(country_code)),
    CONSTRAINT chk_region_policy_auto_moderation_level
        CHECK (auto_moderation_level IN ('relaxed', 'standard', 'strict'))
);

CREATE INDEX IF NOT EXISTS idx_region_policies_discoverability
    ON region_policies(discoverability_enabled);

INSERT INTO region_policies (country_code)
SELECT DISTINCT UPPER(country_code)
FROM cities
WHERE country_code IS NOT NULL AND country_code <> ''
ON CONFLICT (country_code) DO NOTHING;
