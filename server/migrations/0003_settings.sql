-- The Owner's settings, one row per key. A missing row means the default
-- (e.g. no 'path_template' row = the "Short with extension" Preset).
CREATE TABLE settings (
    key        TEXT    PRIMARY KEY,
    value      TEXT    NOT NULL,
    updated_at INTEGER NOT NULL -- unix seconds
);
