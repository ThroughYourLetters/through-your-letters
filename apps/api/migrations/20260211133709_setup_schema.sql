CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "postgis";

-- 1. Cities
CREATE TABLE cities (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    country_code VARCHAR(2) NOT NULL,
    center_lat DOUBLE PRECISION DEFAULT 12.9716,
    center_lng DOUBLE PRECISION DEFAULT 77.5946,
    default_zoom INTEGER DEFAULT 12,
    description TEXT,
    is_active BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 2. Letterings (Artifacts)
CREATE TABLE letterings (
    id UUID PRIMARY KEY,
    city_id UUID NOT NULL REFERENCES cities(id),
    contributor_tag VARCHAR(30) NOT NULL,
    image_url TEXT NOT NULL,
    thumbnail_small TEXT NOT NULL,
    thumbnail_medium TEXT NOT NULL,
    thumbnail_large TEXT NOT NULL,
    location GEOGRAPHY(POINT, 4326) NOT NULL,
    pin_code VARCHAR(6) NOT NULL,
    detected_text TEXT,
    description TEXT,
    image_hash VARCHAR(64) UNIQUE,
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    ml_style VARCHAR(50),
    ml_script VARCHAR(50),
    ml_confidence REAL,
    ml_color_palette JSONB DEFAULT '[]'::jsonb,
    cultural_context TEXT,
    report_count INTEGER NOT NULL DEFAULT 0,
    report_reasons JSONB NOT NULL DEFAULT '[]'::jsonb,
    likes_count INTEGER NOT NULL DEFAULT 0,
    comments_count INTEGER NOT NULL DEFAULT 0,
    uploaded_by_ip INET,
    detected_text_tsv tsvector,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 3. Social
CREATE TABLE likes (
    id UUID PRIMARY KEY,
    lettering_id UUID NOT NULL REFERENCES letterings(id) ON DELETE CASCADE,
    user_ip INET NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(lettering_id, user_ip)
);

CREATE TABLE comments (
    id UUID PRIMARY KEY,
    lettering_id UUID NOT NULL REFERENCES letterings(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    user_ip INET,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 4. Stats & Community
CREATE TABLE daily_stats (
    date DATE PRIMARY KEY,
    uploads_count INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE collections (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    creator_tag VARCHAR(30) NOT NULL,
    is_public BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE collection_items (
    collection_id UUID REFERENCES collections(id) ON DELETE CASCADE,
    lettering_id UUID REFERENCES letterings(id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, lettering_id)
);

-- 5. Search Indexing
CREATE INDEX idx_letterings_fts ON letterings USING gin(detected_text_tsv);
CREATE INDEX idx_letterings_location ON letterings USING GIST(location);

CREATE OR REPLACE FUNCTION update_lettering_tsv() RETURNS trigger AS $$
BEGIN
    NEW.detected_text_tsv := to_tsvector('english', COALESCE(NEW.detected_text, '') || ' ' || COALESCE(NEW.description, ''));
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER letterings_tsv_update BEFORE INSERT OR UPDATE ON letterings
FOR EACH ROW EXECUTE FUNCTION update_lettering_tsv();

-- 6. Initial Data
INSERT INTO cities (id, name, country_code, is_active) 
VALUES ('0194f123-4567-7abc-8def-0123456789ab', 'Bengaluru', 'IN', true);