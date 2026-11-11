CREATE TABLE public_cache_revision (
    singleton BOOLEAN PRIMARY KEY DEFAULT true CHECK (singleton),
    epoch UUID NOT NULL DEFAULT gen_random_uuid()
);
INSERT INTO public_cache_revision(singleton) VALUES (true);

CREATE TABLE public_cache_writers (
    backend_pid INTEGER PRIMARY KEY CHECK (backend_pid > 0),
    revision BIGINT NOT NULL CHECK (revision > 0)
);

CREATE FUNCTION bump_public_cache_revision() RETURNS trigger AS $$
BEGIN
    -- A backend runs one transaction at a time; writers never share a counter row.
    INSERT INTO public_cache_writers(backend_pid, revision) VALUES (pg_backend_pid(), 1)
    ON CONFLICT (backend_pid) DO UPDATE
        SET revision = public_cache_writers.revision + 1;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- The revision becomes visible with the same commit as each public-data change.
CREATE TRIGGER letterings_public_cache_revision
    AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON letterings
    FOR EACH STATEMENT EXECUTE FUNCTION bump_public_cache_revision();
CREATE TRIGGER cities_public_cache_revision
    AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON cities
    FOR EACH STATEMENT EXECUTE FUNCTION bump_public_cache_revision();
CREATE TRIGGER policies_public_cache_revision
    AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON region_policies
    FOR EACH STATEMENT EXECUTE FUNCTION bump_public_cache_revision();
