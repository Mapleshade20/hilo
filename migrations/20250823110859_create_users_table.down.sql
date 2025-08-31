-- Add down migration script here
DROP TRIGGER IF EXISTS set_timestamp ON users;
DROP TABLE IF EXISTS users;
DROP TYPE IF EXISTS user_status;
DROP FUNCTION IF EXISTS trigger_set_timestamp;
