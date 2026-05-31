-- Minimal v2_creator_fee_vault_stats table for test DB
-- Mirrors migrations/vault.sql definition
CREATE TABLE IF NOT EXISTS v2_creator_fee_vault_stats (
    token_id VARCHAR(42) PRIMARY KEY,
    current_balance NUMERIC NOT NULL DEFAULT 0,
    total_deposited NUMERIC NOT NULL DEFAULT 0,
    total_deposited_usd NUMERIC NOT NULL DEFAULT 0,
    total_claimed NUMERIC NOT NULL DEFAULT 0,
    total_claimed_usd NUMERIC NOT NULL DEFAULT 0,
    deposit_count INT NOT NULL DEFAULT 0,
    claim_count INT NOT NULL DEFAULT 0,
    last_block BIGINT NOT NULL DEFAULT 0,
    updated_at BIGINT NOT NULL DEFAULT 0
);
