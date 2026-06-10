DO $$
DECLARE c text;
BEGIN
    SELECT conname INTO c FROM pg_constraint
    WHERE conrelid = 'logs'::regclass AND contype = 'c';
    IF c IS NOT NULL THEN
        EXECUTE format('ALTER TABLE logs DROP CONSTRAINT %I', c);
    END IF;
END $$;

ALTER TABLE logs ADD CONSTRAINT logs_parsed_type_check
    CHECK (parsed_type IN ('nutrition', 'person', 'album', 'song'));
