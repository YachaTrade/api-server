-- account_swap_count 테이블 생성 및 트리거 설정
-- 계정별 swap 카운트를 미리 계산하여 성능 개선 (371ms → 0.04ms)

-- 1. account_swap_count 테이블 생성
CREATE TABLE IF NOT EXISTS account_swap_count (
    account_id TEXT PRIMARY KEY,
    total_count BIGINT NOT NULL DEFAULT 0,
    buy_count BIGINT NOT NULL DEFAULT 0,
    sell_count BIGINT NOT NULL DEFAULT 0
);

-- 2. 기존 데이터로 초기화
INSERT INTO account_swap_count (account_id, total_count, buy_count, sell_count)
SELECT 
    account_id,
    COUNT(*) as total_count,
    COUNT(CASE WHEN is_buy THEN 1 END) as buy_count,
    COUNT(CASE WHEN NOT is_buy THEN 1 END) as sell_count
FROM swap
GROUP BY account_id
ON CONFLICT (account_id) DO NOTHING;

-- 3. 트리거 함수 생성
CREATE OR REPLACE FUNCTION update_account_swap_count()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        -- INSERT 시: 카운트 증가
        INSERT INTO account_swap_count (account_id, total_count, buy_count, sell_count)
        VALUES (
            NEW.account_id, 
            1, 
            CASE WHEN NEW.is_buy THEN 1 ELSE 0 END,
            CASE WHEN NOT NEW.is_buy THEN 1 ELSE 0 END
        )
        ON CONFLICT (account_id) DO UPDATE SET
            total_count = account_swap_count.total_count + 1,
            buy_count = account_swap_count.buy_count + CASE WHEN NEW.is_buy THEN 1 ELSE 0 END,
            sell_count = account_swap_count.sell_count + CASE WHEN NOT NEW.is_buy THEN 1 ELSE 0 END;
            
    ELSIF TG_OP = 'DELETE' THEN
        -- DELETE 시: 카운트 감소 (0 이하로 가지 않도록 보호)
        UPDATE account_swap_count SET
            total_count = GREATEST(0, total_count - 1),
            buy_count = GREATEST(0, buy_count - CASE WHEN OLD.is_buy THEN 1 ELSE 0 END),
            sell_count = GREATEST(0, sell_count - CASE WHEN NOT OLD.is_buy THEN 1 ELSE 0 END)
        WHERE account_id = OLD.account_id;
        
    ELSIF TG_OP = 'UPDATE' THEN
        -- UPDATE 시: is_buy가 변경되었을 경우 처리
        IF OLD.is_buy != NEW.is_buy THEN
            UPDATE account_swap_count SET
                buy_count = buy_count + CASE WHEN NEW.is_buy THEN 1 ELSE -1 END,
                sell_count = sell_count + CASE WHEN NEW.is_buy THEN -1 ELSE 1 END
            WHERE account_id = NEW.account_id;
        END IF;
        
        -- account_id가 변경되었을 경우 처리
        IF OLD.account_id != NEW.account_id THEN
            -- 기존 계정에서 감소
            UPDATE account_swap_count SET
                total_count = GREATEST(0, total_count - 1),
                buy_count = GREATEST(0, buy_count - CASE WHEN OLD.is_buy THEN 1 ELSE 0 END),
                sell_count = GREATEST(0, sell_count - CASE WHEN NOT OLD.is_buy THEN 1 ELSE 0 END)
            WHERE account_id = OLD.account_id;
            
            -- 새 계정에 추가
            INSERT INTO account_swap_count (account_id, total_count, buy_count, sell_count)
            VALUES (
                NEW.account_id, 
                1, 
                CASE WHEN NEW.is_buy THEN 1 ELSE 0 END,
                CASE WHEN NOT NEW.is_buy THEN 1 ELSE 0 END
            )
            ON CONFLICT (account_id) DO UPDATE SET
                total_count = account_swap_count.total_count + 1,
                buy_count = account_swap_count.buy_count + CASE WHEN NEW.is_buy THEN 1 ELSE 0 END,
                sell_count = account_swap_count.sell_count + CASE WHEN NOT NEW.is_buy THEN 1 ELSE 0 END;
        END IF;
    END IF;
    
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- 4. 트리거 생성
DROP TRIGGER IF EXISTS trigger_update_account_swap_count ON swap;
CREATE TRIGGER trigger_update_account_swap_count
AFTER INSERT OR UPDATE OR DELETE ON swap
FOR EACH ROW
EXECUTE FUNCTION update_account_swap_count();

-- 5. 데이터 검증
-- 몇 개의 계정에 대해 실제 swap 수와 account_swap_count가 일치하는지 확인
WITH actual_counts AS (
    SELECT 
        account_id,
        COUNT(*) as actual_total,
        COUNT(CASE WHEN is_buy THEN 1 END) as actual_buy,
        COUNT(CASE WHEN NOT is_buy THEN 1 END) as actual_sell
    FROM swap
    WHERE account_id IN (
        SELECT account_id FROM account_swap_count LIMIT 5
    )
    GROUP BY account_id
),
stored_counts AS (
    SELECT 
        account_id,
        total_count as stored_total,
        buy_count as stored_buy,
        sell_count as stored_sell
    FROM account_swap_count
    WHERE account_id IN (
        SELECT account_id FROM account_swap_count LIMIT 5
    )
)
SELECT 
    a.account_id,
    a.actual_total = s.stored_total as total_match,
    a.actual_buy = s.stored_buy as buy_match,
    a.actual_sell = s.stored_sell as sell_match
FROM actual_counts a
JOIN stored_counts s ON a.account_id = s.account_id;

-- 6. 성능 테스트 쿼리
-- 최적화 전: 371ms
-- SELECT COALESCE(COUNT(*)::bigint, 0) as count FROM swap WHERE account_id = $1;

-- 최적화 후: 0.04ms
-- SELECT COALESCE(total_count, 0) as count FROM account_swap_count WHERE account_id = $1;