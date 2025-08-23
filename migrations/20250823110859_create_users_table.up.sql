-- Add up migration script here
-- Add migration script here
CREATE TYPE user_status AS ENUM (
    'unverified',               -- initial state: email not verified
    'verified',                 -- email verified, but questionnaire not completed
    'questionnaire_completed',  -- questionnaire completed, waiting to be matched
    'matched',                  -- matched pair generated, awaiting confirmation from both parties
    'confirmed'                 -- match confirmed
);
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    status user_status NOT NULL DEFAULT 'unverified',
    wechat_id VARCHAR(100),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE OR REPLACE FUNCTION trigger_set_timestamp()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_timestamp
BEFORE UPDATE ON users
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();