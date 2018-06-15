-- The following SQL is copied from https://github.com/diesel-rs/diesel/tree/v1.3.0/examples/sqlite/
CREATE TABLE posts (
    id INTEGER NOT NULL PRIMARY KEY,
    title VARCHAR NOT NULL,
    body TEXT NOT NULL,
    published BOOLEAN NOT NULL DEFAULT 0
)
