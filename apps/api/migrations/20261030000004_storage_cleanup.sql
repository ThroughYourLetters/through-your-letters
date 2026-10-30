CREATE TABLE storage_cleanup (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    object_url TEXT NOT NULL UNIQUE CHECK (octet_length(object_url) BETWEEN 1 AND 4096),
    state TEXT NOT NULL DEFAULT 'ready' CHECK (state IN ('ready', 'running', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 100),
    available_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    lease_token UUID,
    lease_expires_at TIMESTAMPTZ,
    last_error_code TEXT CHECK (last_error_code IN ('storage_unavailable', 'unrecognized_url')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK ((state = 'running') = (lease_token IS NOT NULL)),
    CHECK ((state = 'running') = (lease_expires_at IS NOT NULL))
);

CREATE INDEX storage_cleanup_ready ON storage_cleanup(available_at, id) WHERE state='ready';
CREATE INDEX storage_cleanup_expired ON storage_cleanup(lease_expires_at, id) WHERE state='running';

-- Fixed-size index entries cover references in every image field. Readers also
-- compare the original URL, so a hash collision cannot authorize deletion.
CREATE INDEX letterings_storage_references ON letterings USING GIN
((ARRAY[md5(image_url), md5(thumbnail_small), md5(thumbnail_medium), md5(thumbnail_large)]));

CREATE FUNCTION queue_deleted_lettering_objects() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO storage_cleanup(object_url)
    SELECT DISTINCT object_url FROM unnest(ARRAY[
        OLD.image_url, OLD.thumbnail_small, OLD.thumbnail_medium, OLD.thumbnail_large
    ]) AS objects(object_url)
    WHERE object_url IS NOT NULL AND object_url <> ''
    ON CONFLICT (object_url) DO UPDATE SET
        available_at=LEAST(storage_cleanup.available_at, clock_timestamp());
    RETURN OLD;
END;
$$;

CREATE TRIGGER queue_deleted_lettering_objects
AFTER DELETE ON letterings FOR EACH ROW EXECUTE FUNCTION queue_deleted_lettering_objects();
