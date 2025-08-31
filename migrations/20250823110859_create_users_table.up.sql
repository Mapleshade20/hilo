-- Add up migration script here
CREATE TYPE user_status AS ENUM (
    'unverified',               -- email verified, but card not verified
    'verification_pending',     -- email verified, card photo uploaded, awaiting admin verification
    'verified',                 -- email and card verified, but form not completed
    'form_completed',           -- form completed, waiting to be matched
    'matched',                  -- matched pair generated, awaiting confirmation from both parties
    'confirmed'                 -- match confirmed
);
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    status user_status NOT NULL DEFAULT 'unverified',
    wechat_id VARCHAR(100),
    card_photo_path VARCHAR(255),
    grade VARCHAR(50),

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
