-- holder_count 최적화를 위한 테이블 및 트리거 생성

-- 1. holder_count를 저장할 테이블 생성
CREATE TABLE IF NOT EXISTS token_holder_count (
    token_id TEXT PRIMARY KEY,
    holder_count BIGINT NOT NULL DEFAULT 0,
    last_updated TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT fk_token_id FOREIGN KEY (token_id) REFERENCES token(token_id) ON DELETE CASCADE
);

-- 인덱스 생성
CREATE INDEX idx_token_holder_count_updated ON token_holder_count(last_updated);

-- 2. 초기 데이터 삽입 (기존 토큰들의 holder_count 계산)
INSERT INTO token_holder_count (token_id, holder_count)
SELECT 
    t.token_id,
    COALESCE(COUNT(b.account_id), 0) as holder_count
FROM token t
LEFT JOIN balance b ON t.token_id = b.token_id AND b.balance > 0
GROUP BY t.token_id
ON CONFLICT (token_id) DO UPDATE 
SET holder_count = EXCLUDED.holder_count,
    last_updated = NOW();

-- 3. holder_count 업데이트 함수
CREATE OR REPLACE FUNCTION update_token_holder_count() RETURNS TRIGGER AS $$
BEGIN
    -- INSERT 또는 UPDATE로 balance가 0보다 커진 경우
    IF TG_OP = 'INSERT' OR (TG_OP = 'UPDATE' AND OLD.balance <= 0 AND NEW.balance > 0) THEN
        UPDATE token_holder_count 
        SET holder_count = holder_count + 1,
            last_updated = NOW()
        WHERE token_id = NEW.token_id;
        
        -- 테이블에 없으면 삽입
        IF NOT FOUND THEN
            INSERT INTO token_holder_count (token_id, holder_count)
            VALUES (NEW.token_id, 1)
            ON CONFLICT (token_id) DO UPDATE 
            SET holder_count = token_holder_count.holder_count + 1,
                last_updated = NOW();
        END IF;
        
    -- UPDATE로 balance가 0 이하로 변경된 경우
    ELSIF TG_OP = 'UPDATE' AND OLD.balance > 0 AND NEW.balance <= 0 THEN
        UPDATE token_holder_count 
        SET holder_count = GREATEST(holder_count - 1, 0),
            last_updated = NOW()
        WHERE token_id = NEW.token_id;
        
    -- DELETE로 balance > 0인 레코드가 삭제된 경우
    ELSIF TG_OP = 'DELETE' AND OLD.balance > 0 THEN
        UPDATE token_holder_count 
        SET holder_count = GREATEST(holder_count - 1, 0),
            last_updated = NOW()
        WHERE token_id = OLD.token_id;
    END IF;
    
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- 4. balance 테이블에 트리거 생성
DROP TRIGGER IF EXISTS trg_update_holder_count ON balance;
CREATE TRIGGER trg_update_holder_count
AFTER INSERT OR UPDATE OR DELETE ON balance
FOR EACH ROW
EXECUTE FUNCTION update_token_holder_count();

-- 5. 새로운 토큰이 생성될 때 holder_count 테이블에도 추가하는 함수
CREATE OR REPLACE FUNCTION insert_token_holder_count() RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO token_holder_count (token_id, holder_count)
    VALUES (NEW.token_id, 0)
    ON CONFLICT (token_id) DO NOTHING;
    
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- 6. token 테이블에 트리거 생성
DROP TRIGGER IF EXISTS trg_insert_token_holder_count ON token;
CREATE TRIGGER trg_insert_token_holder_count
AFTER INSERT ON token
FOR EACH ROW
EXECUTE FUNCTION insert_token_holder_count();

-- 7. 데이터 정합성 검증 함수 (주기적으로 실행 가능)
CREATE OR REPLACE FUNCTION verify_holder_counts() RETURNS TABLE(
    token_id TEXT,
    calculated_count BIGINT,
    stored_count BIGINT,
    difference BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        t.token_id,
        COUNT(b.account_id)::BIGINT as calculated_count,
        COALESCE(thc.holder_count, 0) as stored_count,
        COUNT(b.account_id)::BIGINT - COALESCE(thc.holder_count, 0) as difference
    FROM token t
    LEFT JOIN balance b ON t.token_id = b.token_id AND b.balance > 0
    LEFT JOIN token_holder_count thc ON t.token_id = thc.token_id
    GROUP BY t.token_id, thc.holder_count
    HAVING COUNT(b.account_id)::BIGINT != COALESCE(thc.holder_count, 0);
END;
$$ LANGUAGE plpgsql;

-- 8. 전체 holder_count 재계산 함수 (필요시 사용)
CREATE OR REPLACE FUNCTION recalculate_all_holder_counts() RETURNS void AS $$
BEGIN
    -- 모든 토큰의 holder_count 재계산
    INSERT INTO token_holder_count (token_id, holder_count)
    SELECT 
        t.token_id,
        COUNT(b.account_id) as holder_count
    FROM token t
    LEFT JOIN balance b ON t.token_id = b.token_id AND b.balance > 0
    GROUP BY t.token_id
    ON CONFLICT (token_id) DO UPDATE 
    SET holder_count = EXCLUDED.holder_count,
        last_updated = NOW();
END;
$$ LANGUAGE plpgsql;

-- 사용 예시:
-- SELECT * FROM token_holder_count WHERE token_id = '0xToken00000000000000000000000000000000001';
-- SELECT * FROM verify_holder_counts() LIMIT 10;
-- SELECT recalculate_all_holder_counts();