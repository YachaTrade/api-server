-- hype.rs 최적화를 위한 추가 인덱스

-- 1. market 테이블에 price 정렬 최적화를 위한 복합 인덱스
-- (기존 idx_market_price는 단일 컬럼 인덱스)
CREATE INDEX IF NOT EXISTS idx_market_token_price 
ON market(token_id, price DESC NULLS LAST);

-- 2. account_x 테이블에 covering index 추가 (LATERAL JOIN 최적화)
-- (기존 account_x_account_id_index는 account_id만 포함)
CREATE INDEX IF NOT EXISTS idx_account_x_covering 
ON account_x(account_id) 
INCLUDE (x_handle, x_image_uri, is_blue_label);

-- 3. balance 테이블 최적화 (이미 idx_balance_token_positive 존재하므로 생략)

-- 4. hype_token 테이블 인덱스 (이미 idx_hype_token_token_id 존재하므로 생략)

-- 인덱스 생성 상태 확인
SELECT 
    schemaname,
    tablename,
    indexname,
    indexdef
FROM pg_indexes
WHERE schemaname = 'public'
AND tablename IN ('market', 'account_x', 'balance', 'hype_token')
ORDER BY tablename, indexname;