-- API Tokens: named, revocable secrets that let a tool upload for the Owner.
--
-- Only the SHA-256 of the secret is stored (the secret is 32 random bytes,
-- so a fast hash is enough; there is nothing to brute-force). A revoked
-- token keeps its row so the admin area can show it as revoked; its name is
-- free again for a new token.
CREATE TABLE api_tokens (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT    NOT NULL,
    secret_hash  TEXT    NOT NULL UNIQUE, -- lower-case hex SHA-256 of the secret
    created_at   INTEGER NOT NULL,        -- unix seconds
    last_used_at INTEGER,                 -- unix seconds; NULL until first used
    revoked_at   INTEGER                  -- unix seconds; NULL while active
);

-- Names are unique among active tokens.
CREATE UNIQUE INDEX api_tokens_active_name ON api_tokens (name) WHERE revoked_at IS NULL;
