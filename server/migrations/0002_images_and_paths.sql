-- Images: one row per distinct file content, identified by its SHA-256.
-- The bytes live on disk under the data directory, named by the hash.
CREATE TABLE images (
    id                INTEGER PRIMARY KEY,
    sha256            TEXT    NOT NULL UNIQUE, -- lower-case hex
    size              INTEGER NOT NULL,        -- bytes
    mime              TEXT    NOT NULL,        -- detected type, e.g. image/png
    width             INTEGER,                 -- NULL when unknown (some SVGs)
    height            INTEGER,
    original_filename TEXT    NOT NULL,
    uploaded_at       INTEGER NOT NULL,        -- unix seconds
    trashed_at        INTEGER                  -- unix seconds; NULL unless in the Trash
);

CREATE INDEX images_uploaded_at ON images (uploaded_at);

-- Paths: the part of a public link after the domain, without the leading
-- '/'. Unique across all Images (trashed ones included) and matched exactly
-- (BINARY collation, so case-sensitive).
CREATE TABLE paths (
    id         INTEGER PRIMARY KEY,
    path       TEXT    NOT NULL UNIQUE,
    image_id   INTEGER NOT NULL REFERENCES images (id) ON DELETE CASCADE,
    is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    created_at INTEGER NOT NULL -- unix seconds
);

CREATE INDEX paths_image_id ON paths (image_id);
-- At most one Primary Path per Image.
CREATE UNIQUE INDEX paths_one_primary ON paths (image_id) WHERE is_primary = 1;
