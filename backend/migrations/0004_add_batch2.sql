ALTER TABLE logs DROP CONSTRAINT logs_parsed_type_check;
ALTER TABLE logs ADD CONSTRAINT logs_parsed_type_check
    CHECK (parsed_type IN ('nutrition', 'person', 'album', 'song', 'workout',
                           'learning', 'place', 'trip', 'sleep'));

CREATE TABLE fields (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name          text NOT NULL,
    goal_text     text,
    timeline_text text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    archived_at   timestamptz
);

CREATE TABLE resources (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    field_id     uuid NOT NULL REFERENCES fields(id) ON DELETE CASCADE,
    kind         text NOT NULL CHECK (kind IN ('pdf', 'url', 'manual')),
    title        text NOT NULL,
    uri          text,
    total_units  int,
    unit_label   text,
    current_unit int NOT NULL DEFAULT 0,
    structure    text,
    created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE resource_files (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    resource_id  uuid NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    content_type text NOT NULL,
    bytes        bytea NOT NULL
);

CREATE TABLE topics (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    field_id           uuid NOT NULL REFERENCES fields(id) ON DELETE CASCADE,
    name               text NOT NULL,
    ord                int NOT NULL,
    status             text NOT NULL DEFAULT 'todo'
                       CHECK (status IN ('todo', 'in_progress', 'done')),
    confidence         int CHECK (confidence BETWEEN 1 AND 5),
    source_resource_id uuid REFERENCES resources(id) ON DELETE SET NULL,
    created_at         timestamptz NOT NULL DEFAULT now()
);
