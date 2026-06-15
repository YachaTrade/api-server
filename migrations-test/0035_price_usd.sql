-- Test mirror of migrations/0034_price_usd.sql (DefiLlama per-token USD price).
CREATE TABLE IF NOT EXISTS price_usd (
    token_id     VARCHAR(42) NOT NULL,
    block_number BIGINT      NOT NULL,
    price        NUMERIC     NOT NULL,
    confidence   NUMERIC,
    created_at   BIGINT      NOT NULL,
    PRIMARY KEY (token_id, block_number)
);
CREATE INDEX IF NOT EXISTS idx_price_usd_token_block ON price_usd (token_id, block_number DESC);
