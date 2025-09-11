-- reward_add_history_count 테이블 생성
CREATE TABLE IF NOT EXISTS reward_add_history_count(
    account_id VARCHAR(42) PRIMARY KEY REFERENCES account(account_id),
    total_count BIGINT NOT NULL DEFAULT 0,
    updated_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)::BIGINT
);

-- reward_add_history INSERT 시 카운트 증가 트리거 함수
CREATE OR REPLACE FUNCTION update_reward_add_history_count()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO reward_add_history_count (account_id, total_count, updated_at)
    VALUES (NEW.account_id, 1, EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)::BIGINT)
    ON CONFLICT (account_id) 
    DO UPDATE SET 
        total_count = reward_add_history_count.total_count + 1,
        updated_at = EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)::BIGINT;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 트리거 생성
CREATE TRIGGER trigger_update_reward_add_history_count
    AFTER INSERT ON reward_add_history
    FOR EACH ROW
    EXECUTE FUNCTION update_reward_add_history_count();

-- 기존 데이터에 대한 초기 카운트 계산 (필요한 경우)
INSERT INTO reward_add_history_count (account_id, total_count, updated_at)
SELECT 
    account_id,
    COUNT(*) as total_count,
    EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)::BIGINT as updated_at
FROM reward_add_history
GROUP BY account_id
ON CONFLICT (account_id) DO NOTHING;