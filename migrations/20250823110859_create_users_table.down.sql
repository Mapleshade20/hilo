-- Add down migration script here
DROP TABLE IF EXISTS users;
DROP TYPE IF EXISTS user_status;
DROP TRIGGER IF EXISTS set_timestamp ON users;
DROP FUNCTION IF EXISTS trigger_set_timestamp;