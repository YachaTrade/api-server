-- Standalone test schema for the dividend APIs (subset of migrations/dividend.sql
-- + v2_creator_fee_allocation from migrations/vault.sql). Tables only — triggers
-- and backfill are omitted; tests insert into the aggregate tables directly.

-- ── On-chain dividend tables ────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS v2_dividend_setups (
    source_token     VARCHAR(42) NOT NULL,
    dividend_token   VARCHAR(42) NOT NULL,
    ratio            INT NOT NULL,
    min_balance      NUMERIC NOT NULL,
    entry_index      INT NOT NULL,
    transaction_hash VARCHAR NOT NULL,
    block_number     BIGINT NOT NULL,
    created_at       BIGINT NOT NULL,
    log_index        INT NOT NULL,
    tx_index         INT NOT NULL,
    PRIMARY KEY (transaction_hash, tx_index, log_index, entry_index)
);

CREATE TABLE IF NOT EXISTS v2_dividend_claims (
    holder           VARCHAR(42) NOT NULL,
    source_token     VARCHAR(42) NOT NULL,
    dividend_token   VARCHAR(42) NOT NULL,
    amount           NUMERIC NOT NULL CHECK (amount > 0),
    merkle_root      VARCHAR(66),
    entry_index      INT NOT NULL,
    transaction_hash VARCHAR NOT NULL,
    block_number     BIGINT NOT NULL,
    created_at       BIGINT NOT NULL,
    log_index        INT NOT NULL,
    tx_index         INT NOT NULL,
    usd_value        NUMERIC NOT NULL DEFAULT 0,
    PRIMARY KEY (transaction_hash, tx_index, log_index, entry_index)
);

CREATE TABLE IF NOT EXISTS v2_dividend_vault_stats (
    source_token             VARCHAR(42) NOT NULL,
    dividend_token           VARCHAR(42) NOT NULL,
    total_deposited          NUMERIC NOT NULL DEFAULT 0,
    total_deposited_usd      NUMERIC NOT NULL DEFAULT 0,
    total_pending_deposited  NUMERIC NOT NULL DEFAULT 0,
    total_pending_deposited_usd NUMERIC NOT NULL DEFAULT 0,
    total_consumed_quote     NUMERIC NOT NULL DEFAULT 0,
    total_converted_received NUMERIC NOT NULL DEFAULT 0,
    pending_swap_balance     NUMERIC GENERATED ALWAYS AS
                                 (total_pending_deposited - total_consumed_quote) STORED,
    dividend_balance         NUMERIC NOT NULL DEFAULT 0,
    total_claimed            NUMERIC NOT NULL DEFAULT 0,
    total_claimed_usd        NUMERIC NOT NULL DEFAULT 0,
    claim_count              INT NOT NULL DEFAULT 0,
    last_block               BIGINT NOT NULL DEFAULT 0,
    updated_at               BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (source_token, dividend_token)
);

-- ── Scheduler accrual / distribution tables ─────────────────────────────────
CREATE TABLE IF NOT EXISTS dividend_accrual (
    source_token   VARCHAR(42) NOT NULL,
    holder         VARCHAR(42) NOT NULL,
    dividend_token VARCHAR(42) NOT NULL,
    accrued        NUMERIC(78,0) NOT NULL DEFAULT 0 CHECK (accrued >= 0),
    updated_at     BIGINT  NOT NULL DEFAULT 0,
    PRIMARY KEY (source_token, holder, dividend_token)
);

CREATE TABLE IF NOT EXISTS dividend_pair_state (
    source_token           VARCHAR(42) NOT NULL,
    dividend_token         VARCHAR(42) NOT NULL,
    last_allocated_balance NUMERIC(78,0) NOT NULL DEFAULT 0 CHECK (last_allocated_balance >= 0),
    last_snapshot_block    BIGINT  NOT NULL DEFAULT 0,
    updated_at             BIGINT  NOT NULL DEFAULT 0,
    PRIMARY KEY (source_token, dividend_token)
);

CREATE TABLE IF NOT EXISTS dividend_distribution (
    source_token   VARCHAR(42) NOT NULL,
    holder         VARCHAR(42) NOT NULL,
    dividend_token VARCHAR(42) NOT NULL,
    merkle_root    VARCHAR(66) NOT NULL,
    amount         NUMERIC(78,0) NOT NULL CHECK (amount >= 0),
    proof          TEXT[]  NOT NULL,
    status         VARCHAR NOT NULL CHECK (status IN ('AWAITING', 'CLAIMED')),
    created_at     BIGINT  NOT NULL,
    PRIMARY KEY (source_token, holder, dividend_token)
);

-- ── Fee-split allocation (identifies the "Dividend %") ───────────────────────
CREATE TABLE IF NOT EXISTS v2_creator_fee_allocation (
    token_id VARCHAR(42) NOT NULL,
    vault_id VARCHAR(42) NOT NULL,
    bps INT NOT NULL CHECK (bps >= 0 AND bps <= 10000),
    transaction_hash VARCHAR NOT NULL,
    block_number BIGINT NOT NULL,
    created_at BIGINT NOT NULL,
    log_index INT NOT NULL,
    tx_index INT NOT NULL,
    PRIMARY KEY (token_id, vault_id)
);
