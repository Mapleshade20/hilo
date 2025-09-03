-- Add up migration script here
CREATE TABLE vetoes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    vetoer_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    vetoed_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    -- Ensure unique vetoes and prevent self-vetoing
    UNIQUE(vetoer_id, vetoed_id),
    CHECK(vetoer_id != vetoed_id)
);
