-- whitelist_token: Select Token 모달 고정순서 화이트리스트.
-- 렌더 메타는 token/dex_token/quote_token LEFT JOIN. seed는 프로덕션 마이그레이션에만.
CREATE TABLE IF NOT EXISTS whitelist_token (
    token_id   VARCHAR(42) PRIMARY KEY,
    sort_order INT NOT NULL,
    enabled    BOOLEAN NOT NULL DEFAULT TRUE
);
