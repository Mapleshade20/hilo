-- Add down migration script here
DROP TRIGGER IF EXISTS set_timestamp ON forms;
DROP TABLE IF EXISTS forms;
DROP TYPE IF EXISTS gender;
