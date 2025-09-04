-- Add up migration script here
CREATE TYPE gender AS ENUM ('male', 'female');

CREATE TABLE forms (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID UNIQUE NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Section A
    gender gender NOT NULL,
    -- wechat_id goes here in frontend but is stored in users table

    -- Section B
    -- B1: familiar hobbies
    familiar_tags TEXT[] NOT NULL,
    -- B2: aspirational hobbies
    aspirational_tags TEXT[] NOT NULL,
    -- B3: recent topics of interest
    recent_topics TEXT NOT NULL,
    -- B4: personalities
    self_traits TEXT[] NOT NULL,
    ideal_traits TEXT[] NOT NULL,
    -- B5: acceptance of physical contact (1~4: 1=avoid, 4=comfortable)
    physical_boundary SMALLINT NOT NULL,

    -- Section C
    self_intro TEXT NOT NULL,
    profile_photo_path VARCHAR(255),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TRIGGER set_timestamp
BEFORE UPDATE ON forms
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();
