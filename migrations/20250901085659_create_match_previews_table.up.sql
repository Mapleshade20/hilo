-- Add up migration script here
CREATE TABLE match_previews (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID UNIQUE NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    candidate_ids UUID[] NOT NULL,

    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TRIGGER set_timestamp_match_previews
BEFORE UPDATE ON match_previews
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();
