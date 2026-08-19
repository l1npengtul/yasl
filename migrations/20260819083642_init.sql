-- Add migration script here
PRAGMA foreign_keys = on;

CREATE TABLE IF NOT EXISTS banned_tags(
    server_id INTEGER,
    tag TEXT
) STRICT;

CREATE INDEX idx_banned_tags_server_id ON banned_tags(server_id);
CREATE INDEX idx_banned_tags_tag ON banned_tags(tag);

CREATE TABLE IF NOT EXISTS exempted_users(
    user_id INTEGER NOT NULL UNIQUE,
    timestamp INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_exempted_users_user_id ON exempted_users(user_id);

CREATE TABLE IF NOT EXISTS actions(
    action_done TEXT NOT NULL,
    user_id INTEGER NOT NULL,
    timestamp INTEGER NOT NULL
) STRICT;
