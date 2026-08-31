ALTER TABLE playlist_tracks
    ADD COLUMN audio_size_bytes BIGINT,
    ADD COLUMN audio_sha256 CHAR(64),
    ADD COLUMN audio_updated_at TIMESTAMPTZ,
    ADD CONSTRAINT playlist_tracks_audio_size_check
        CHECK (audio_size_bytes IS NULL OR audio_size_bytes > 0),
    ADD CONSTRAINT playlist_tracks_audio_sha_check
        CHECK (audio_sha256 IS NULL OR audio_sha256 ~ '^[0-9a-f]{64}$'),
    ADD CONSTRAINT playlist_tracks_audio_coherent_check
        CHECK ((audio_size_bytes IS NULL) = (audio_sha256 IS NULL));
