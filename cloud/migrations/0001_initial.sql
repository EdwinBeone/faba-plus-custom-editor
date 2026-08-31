CREATE TABLE users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    library_version BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (email = LOWER(email)),
    CHECK (char_length(email) BETWEEN 3 AND 254),
    CHECK (char_length(display_name) BETWEEN 1 AND 80)
);
CREATE TABLE sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL UNIQUE,
    client_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX sessions_user_id_idx ON sessions(user_id);
CREATE INDEX sessions_expires_at_idx ON sessions(expires_at);

CREATE TABLE playlists (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    figure_id CHAR(4) NOT NULL,
    name TEXT NOT NULL,
    nfc_payload CHAR(14) NOT NULL,
    track_count SMALLINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, figure_id),
    CHECK (figure_id ~ '^[2-8][0-9]{3}$'),
    CHECK (nfc_payload = '02190530' || figure_id || '00'),
    CHECK (char_length(name) BETWEEN 1 AND 100),
    CHECK (track_count BETWEEN 1 AND 99)
);

CREATE TABLE playlist_tracks (
    user_id UUID NOT NULL,
    figure_id CHAR(4) NOT NULL,
    position SMALLINT NOT NULL,
    label TEXT NOT NULL,
    PRIMARY KEY (user_id, figure_id, position),
    FOREIGN KEY (user_id, figure_id)
        REFERENCES playlists(user_id, figure_id)
        ON DELETE CASCADE,
    CHECK (position BETWEEN 0 AND 98),
    CHECK (char_length(label) BETWEEN 1 AND 200)
);
