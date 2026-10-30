ALTER TABLE analysis_jobs DROP CONSTRAINT analysis_jobs_last_error_code_check;
ALTER TABLE analysis_jobs ADD CONSTRAINT analysis_jobs_last_error_code_check
CHECK (last_error_code IN ('processing_failed','timed_out','lease_expired','interrupted'));
