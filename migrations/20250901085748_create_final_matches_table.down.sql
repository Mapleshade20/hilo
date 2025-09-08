-- Add down migration script here
DROP INDEX IF EXISTS idx_final_matches_user_a;
DROP INDEX IF EXISTS idx_final_matches_user_b;
DROP TABLE IF EXISTS final_matches;
