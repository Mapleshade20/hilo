-- Add down migration script here
DROP TRIGGER IF EXISTS set_timestamp ON users;
DROP INDEX IF EXISTS idx_users_status_created_at;
DROP TABLE IF EXISTS users;
DROP TYPE IF EXISTS user_status;
DROP FUNCTION IF EXISTS trigger_set_timestamp;
