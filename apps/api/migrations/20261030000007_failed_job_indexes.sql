CREATE INDEX analysis_jobs_failed ON analysis_jobs(lettering_id) WHERE state='failed';
CREATE INDEX storage_cleanup_problems ON storage_cleanup(id)
WHERE state='failed' OR (state='ready' AND last_error_code IS NOT NULL);
