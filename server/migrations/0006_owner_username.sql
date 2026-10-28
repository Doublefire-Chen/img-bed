-- The Owner's username for logging in, stored as entered (trimmed). Matched
-- case-insensitively in code. NULL on databases from before this migration
-- until `img-bed set-password --username <name>` is run; login is refused
-- until then.
ALTER TABLE owner ADD COLUMN username TEXT;
