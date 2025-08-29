-- Step 1: Add verified_token_count field to token_count table
ALTER TABLE token_count ADD COLUMN IF NOT EXISTS verified_token_count BIGINT NOT NULL DEFAULT 0;

-- Step 2: Initialize verified_token_count with current data
-- account_verified는 x_handle만 가지고 있으므로 JOIN 로직 수정
UPDATE token_count SET verified_token_count = (
    SELECT COUNT(*)
    FROM token t
    JOIN account a ON t.creator = a.account_id
    JOIN account_x ax ON t.creator = ax.account_id
    JOIN account_verified av ON ax.x_handle = av.x_handle
);

-- Step 3: Trigger 1 - When a new token is created
CREATE OR REPLACE FUNCTION update_verified_count_on_token_insert()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    -- Check if the token creator is verified
    -- account_verified에 account_id가 없으므로 x_handle로 확인
    IF EXISTS (
        SELECT 1 
        FROM account a
        JOIN account_x ax ON a.account_id = ax.account_id
        JOIN account_verified av ON ax.x_handle = av.x_handle
        WHERE a.account_id = NEW.creator
    ) THEN
        UPDATE token_count SET verified_token_count = verified_token_count + 1;
    END IF;
    RETURN NEW;
END;
$$;

-- Create trigger for token insert
DROP TRIGGER IF EXISTS token_verified_count_insert_trigger ON token;
CREATE TRIGGER token_verified_count_insert_trigger
    AFTER INSERT ON token
    FOR EACH ROW EXECUTE FUNCTION update_verified_count_on_token_insert();

-- Step 4: Trigger 2 - When account_verified is added (account becomes verified)
-- account_verified는 x_handle만 가지므로 해당 x_handle을 가진 모든 계정의 토큰 카운트
CREATE OR REPLACE FUNCTION update_verified_count_on_verified_insert()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    token_count_for_handle INTEGER;
BEGIN
    -- Count how many tokens are created by accounts with this x_handle
    SELECT COUNT(*) INTO token_count_for_handle
    FROM token t
    JOIN account a ON t.creator = a.account_id
    JOIN account_x ax ON a.account_id = ax.account_id
    WHERE ax.x_handle = NEW.x_handle;
    
    -- Add that count to verified_token_count
    UPDATE token_count SET verified_token_count = verified_token_count + token_count_for_handle;
    
    RETURN NEW;
END;
$$;

-- Create trigger for account_verified insert
DROP TRIGGER IF EXISTS account_verified_insert_trigger ON account_verified;
CREATE TRIGGER account_verified_insert_trigger
    AFTER INSERT ON account_verified
    FOR EACH ROW EXECUTE FUNCTION update_verified_count_on_verified_insert();

-- Step 5: Trigger 3 - When account_verified is removed (account loses verification)
CREATE OR REPLACE FUNCTION update_verified_count_on_verified_delete()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    token_count_for_handle INTEGER;
BEGIN
    -- Count how many tokens are created by accounts with this x_handle
    SELECT COUNT(*) INTO token_count_for_handle
    FROM token t
    JOIN account a ON t.creator = a.account_id
    JOIN account_x ax ON a.account_id = ax.account_id
    WHERE ax.x_handle = OLD.x_handle;
    
    -- Subtract that count from verified_token_count
    UPDATE token_count SET verified_token_count = verified_token_count - token_count_for_handle;
    
    RETURN OLD;
END;
$$;

-- Create trigger for account_verified delete
DROP TRIGGER IF EXISTS account_verified_delete_trigger ON account_verified;
CREATE TRIGGER account_verified_delete_trigger
    AFTER DELETE ON account_verified
    FOR EACH ROW EXECUTE FUNCTION update_verified_count_on_verified_delete();

-- Step 6: Trigger 4 - When account_x x_handle is updated (verification status might change)
CREATE OR REPLACE FUNCTION update_verified_count_on_x_handle_update()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    token_count_for_account INTEGER;
    old_verified BOOLEAN;
    new_verified BOOLEAN;
BEGIN
    -- Skip if x_handle didn't change
    IF OLD.x_handle = NEW.x_handle THEN
        RETURN NEW;
    END IF;
    
    -- Count tokens for this account
    SELECT COUNT(*) INTO token_count_for_account
    FROM token t
    WHERE t.creator = NEW.account_id;
    
    -- Check if account was verified with old x_handle
    SELECT EXISTS(
        SELECT 1 FROM account_verified av WHERE av.x_handle = OLD.x_handle
    ) INTO old_verified;
    
    -- Check if account is verified with new x_handle
    SELECT EXISTS(
        SELECT 1 FROM account_verified av WHERE av.x_handle = NEW.x_handle
    ) INTO new_verified;
    
    -- Update count based on verification status change
    IF old_verified AND NOT new_verified THEN
        -- Lost verification
        UPDATE token_count SET verified_token_count = verified_token_count - token_count_for_account;
    ELSIF NOT old_verified AND new_verified THEN
        -- Gained verification
        UPDATE token_count SET verified_token_count = verified_token_count + token_count_for_account;
    END IF;
    
    RETURN NEW;
END;
$$;

-- Create trigger for account_x x_handle update
DROP TRIGGER IF EXISTS account_x_handle_update_trigger ON account_x;
CREATE TRIGGER account_x_handle_update_trigger
    AFTER UPDATE OF x_handle ON account_x
    FOR EACH ROW EXECUTE FUNCTION update_verified_count_on_x_handle_update();

-- Enable all triggers
ALTER TABLE token ENABLE TRIGGER token_verified_count_insert_trigger;
ALTER TABLE account_verified ENABLE TRIGGER account_verified_insert_trigger;
ALTER TABLE account_verified ENABLE TRIGGER account_verified_delete_trigger;
ALTER TABLE account_x ENABLE TRIGGER account_x_handle_update_trigger;