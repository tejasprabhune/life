CREATE TABLE logs (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at  timestamptz NOT NULL DEFAULT now(),
    raw_input   text NOT NULL,
    parsed_type text NOT NULL CHECK (parsed_type IN ('nutrition', 'person')),
    data        jsonb NOT NULL,
    deleted_at  timestamptz
);

CREATE INDEX logs_created_at_idx ON logs (created_at);
