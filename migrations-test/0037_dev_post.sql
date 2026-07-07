-- migrations-test standalone copy (NOT a symlink): stock Postgres has no pgactive extension,
-- so use plain nextval(). Production uses pgactive.pgactive_snowflake_id_nextval() via migrations/0037.
CREATE SEQUENCE IF NOT EXISTS dev_post_snowflake_seq;

CREATE TABLE IF NOT EXISTS dev_post (
    id          BIGINT PRIMARY KEY DEFAULT nextval('dev_post_snowflake_seq'),
    token_id    VARCHAR(42) NOT NULL,
    author      VARCHAR(42) NOT NULL,
    body        TEXT        NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    edited_at   TIMESTAMPTZ,
    deleted_at  TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_dev_post_token_created ON dev_post (token_id, created_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_dev_post_created       ON dev_post (created_at DESC)            WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_dev_post_author        ON dev_post (author)                     WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS dev_post_image (
    post_id   BIGINT   NOT NULL REFERENCES dev_post(id) ON DELETE CASCADE,
    position  SMALLINT NOT NULL,
    image_uri TEXT     NOT NULL,
    PRIMARY KEY (post_id, position)
);

CREATE TABLE IF NOT EXISTS dev_post_poll (
    post_id   BIGINT      PRIMARY KEY REFERENCES dev_post(id) ON DELETE CASCADE,
    closes_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS dev_post_poll_option (
    post_id   BIGINT   NOT NULL REFERENCES dev_post_poll(post_id) ON DELETE CASCADE,
    position  SMALLINT NOT NULL,
    label     TEXT     NOT NULL,
    image_uri TEXT,
    PRIMARY KEY (post_id, position)
);

CREATE TABLE IF NOT EXISTS dev_post_like (
    post_id    BIGINT      NOT NULL REFERENCES dev_post(id) ON DELETE CASCADE,
    account_id VARCHAR(42) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (post_id, account_id)
);
CREATE INDEX IF NOT EXISTS idx_dev_post_like_created ON dev_post_like (created_at, post_id);

CREATE TABLE IF NOT EXISTS dev_post_poll_vote (
    post_id         BIGINT      NOT NULL REFERENCES dev_post_poll(post_id) ON DELETE CASCADE,
    account_id      VARCHAR(42) NOT NULL,
    option_position SMALLINT    NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (post_id, account_id),
    FOREIGN KEY (post_id, option_position) REFERENCES dev_post_poll_option(post_id, position)
);
