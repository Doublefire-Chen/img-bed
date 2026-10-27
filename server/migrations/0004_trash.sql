-- The Trash.
--
-- Rebuilds `images` with AUTOINCREMENT so the id of a Permanently Deleted
-- Image is never given to a later upload: a stale admin tab (or a bookmark
-- of /admin/library/<id>) must not act on, or show, a different Image.
-- SQLite can't change that in place. Migrations run in a transaction, where
-- foreign keys can't be switched off, so `paths` is rebuilt as well and
-- dropped first: dropping `images` then cascades to nothing.

CREATE TABLE images_new (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    sha256            TEXT    NOT NULL UNIQUE, -- lower-case hex
    size              INTEGER NOT NULL,        -- bytes
    mime              TEXT    NOT NULL,        -- detected type, e.g. image/png
    width             INTEGER,                 -- NULL when unknown (some SVGs)
    height            INTEGER,
    original_filename TEXT    NOT NULL,
    uploaded_at       INTEGER NOT NULL,        -- unix seconds
    trashed_at        INTEGER                  -- unix seconds; NULL unless in the Trash
);
INSERT INTO images_new (id, sha256, size, mime, width, height, original_filename, uploaded_at, trashed_at)
    SELECT id, sha256, size, mime, width, height, original_filename, uploaded_at, trashed_at
    FROM images;

-- Unchanged apart from pointing at the new table (renamed below).
CREATE TABLE paths_new (
    id         INTEGER PRIMARY KEY,
    path       TEXT    NOT NULL UNIQUE,
    image_id   INTEGER NOT NULL REFERENCES images_new (id) ON DELETE CASCADE,
    is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    created_at INTEGER NOT NULL -- unix seconds
);
INSERT INTO paths_new (id, path, image_id, is_primary, created_at)
    SELECT id, path, image_id, is_primary, created_at FROM paths;

DROP TABLE paths;
DROP TABLE images;
-- Renaming also updates the reference in paths_new.
ALTER TABLE images_new RENAME TO images;
ALTER TABLE paths_new RENAME TO paths;

CREATE INDEX images_uploaded_at ON images (uploaded_at);
-- Finds the Images due for Permanent Delete without scanning the Library.
CREATE INDEX images_trashed_at ON images (trashed_at) WHERE trashed_at IS NOT NULL;
CREATE INDEX paths_image_id ON paths (image_id);
-- At most one Primary Path per Image.
CREATE UNIQUE INDEX paths_one_primary ON paths (image_id) WHERE is_primary = 1;
