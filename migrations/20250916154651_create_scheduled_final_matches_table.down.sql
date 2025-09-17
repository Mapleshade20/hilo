-- Drop the scheduled_final_matches table and its enum type
DROP INDEX IF EXISTS idx_scheduled_final_matches_pending;
DROP TABLE IF EXISTS scheduled_final_matches;
DROP TYPE IF EXISTS schedule_status;
