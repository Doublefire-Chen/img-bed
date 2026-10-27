-- The single Owner's credentials. At most one row (id = 1); absent until
-- `img-bed set-password` is run.
CREATE TABLE owner (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    password_hash TEXT    NOT NULL,
    updated_at    INTEGER NOT NULL -- unix seconds
);

-- Owner login sessions. Only a hash of the cookie token is stored.
-- Expiry is fixed at login and never slides.
CREATE TABLE sessions (
    id         INTEGER PRIMARY KEY,
    token_hash TEXT    NOT NULL UNIQUE,
    created_at INTEGER NOT NULL, -- unix seconds
    expires_at INTEGER NOT NULL  -- unix seconds
);

CREATE INDEX sessions_expires_at ON sessions (expires_at);
