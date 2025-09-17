-- Create the scheduled_final_matches table for storing admin-configured automatic final match triggers
CREATE TYPE schedule_status AS ENUM ('pending', 'completed', 'failed');

CREATE TABLE scheduled_final_matches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    scheduled_time TIMESTAMPTZ NOT NULL UNIQUE,
    status schedule_status NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    executed_at TIMESTAMPTZ,
    matches_created INTEGER,
    error_message TEXT
);

-- Index for efficient querying of pending scheduled matches
CREATE INDEX idx_scheduled_final_matches_pending ON scheduled_final_matches(scheduled_time);
