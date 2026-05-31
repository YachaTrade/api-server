-- Minimal v2_gift_vault_stats table for test DB
-- Mirrors migrations/vault.sql definition
CREATE TABLE IF NOT EXISTS v2_gift_vault_stats (
    token_id         VARCHAR(42) PRIMARY KEY,
    current_state    VARCHAR     NOT NULL DEFAULT 'Accumulating',
    current_balance  NUMERIC     NOT NULL DEFAULT 0,
    platform         VARCHAR,
    platform_id      VARCHAR,
    receiver         VARCHAR(42),
    total_deposited  NUMERIC     NOT NULL DEFAULT 0,
    total_deposited_usd NUMERIC  NOT NULL DEFAULT 0,
    total_claimed    NUMERIC     NOT NULL DEFAULT 0,
    total_claimed_usd NUMERIC    NOT NULL DEFAULT 0,
    total_expired    NUMERIC     NOT NULL DEFAULT 0,
    total_expired_usd NUMERIC    NOT NULL DEFAULT 0,
    buyback_quote_spent     NUMERIC NOT NULL DEFAULT 0,
    buyback_quote_spent_usd NUMERIC NOT NULL DEFAULT 0,
    buyback_tokens          NUMERIC NOT NULL DEFAULT 0,
    expires_at       BIGINT      NOT NULL DEFAULT 0,
    receiver_set_at  BIGINT      NOT NULL DEFAULT 0,
    last_block       BIGINT      NOT NULL DEFAULT 0,
    updated_at       BIGINT      NOT NULL DEFAULT 0
);
