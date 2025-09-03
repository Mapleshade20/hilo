-- Add up migration script here
CREATE TABLE final_matches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_a_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    score DOUBLE PRECISION NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Ensure each user only appears once in final matches
    -- and prevent duplicate pairs regardless of order
    CHECK(user_a_id != user_b_id),
    CHECK(user_a_id < user_b_id)
);

-- Index for finding matches by user
CREATE INDEX idx_final_matches_user_a ON final_matches(user_a_id);
CREATE INDEX idx_final_matches_user_b ON final_matches(user_b_id);
