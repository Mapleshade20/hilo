-- Add down migration script here
DROP INDEX IF EXISTS idx_vetoes_vetoer_id;
DROP TABLE IF EXISTS vetoes;
