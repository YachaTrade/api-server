#!/bin/bash

# Load environment variables from .env
if [ -f .env ]; then
    export $(cat .env | grep -v '^#' | xargs)
fi

# Use PRIMARY_DATABASE_URL from .env
DATABASE_URL="${PRIMARY_DATABASE_URL}"

if [ -z "$DATABASE_URL" ]; then
    echo "Error: PRIMARY_DATABASE_URL not found in .env file"
    exit 1
fi

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Starting database dummy data generation...${NC}"
echo -e "${YELLOW}Using database URL from .env${NC}"

# Function to execute SQL
execute_sql() {
    psql "$DATABASE_URL" -c "$1"
}

# Function to execute SQL file
execute_sql_file() {
    psql "$DATABASE_URL" -f "$1"
}

# Clean existing data
echo -e "${YELLOW}Cleaning existing data...${NC}"
execute_sql "TRUNCATE TABLE 
    vote_history,
    reward_airdrop_history,
    reward_pool,
    reward_add_history,
    point_distribution_records,
    point,
    point_history,
    epoch,
    token_management_airdrop_receivers,
    token_management_total_lock,
    token_management_lock,
    token_management_history_count,
    token_management_history,
    creator_vault_history,
    creator_vault_balance,
    buy_back_history,
    buy_back_target,
    buy_back,
    lp_collect_status,
    lp_collect_history,
    lp_allocate_history,
    follow,
    burn_history,
    chart_tx,
    price,
    chart_count,
    chart,
    swap_count,
    swap,
    balance_history,
    balance,
    market,
    king,
    token_count,
    token,
    account_wallet,
    account_x,
    account_session,
    account
    CASCADE;"

# Generate accounts
echo -e "${YELLOW}Generating 10 million accounts...${NC}"

# Generate accounts in batches to avoid memory issues
BATCH_SIZE=100000
TOTAL_ACCOUNTS=10000000

for ((i=1; i<=TOTAL_ACCOUNTS; i+=BATCH_SIZE)); do
    END=$((i+BATCH_SIZE-1))
    if [ $END -gt $TOTAL_ACCOUNTS ]; then
        END=$TOTAL_ACCOUNTS
    fi
    
    echo -e "${GREEN}Generating accounts $i to $END...${NC}"
    
    execute_sql "
    INSERT INTO account (account_id, nickname, bio, image_uri, follower_count, following_count)
    SELECT 
        '0xAccount' || LPAD(j::text, 33, '0') as account_id,
        'user_' || j as nickname,
        'Bio for user ' || j as bio,
        'https://example.com/avatar/' || j || '.png' as image_uri,
        floor(random() * 10000)::int as follower_count,
        floor(random() * 1000)::int as following_count
    FROM generate_series($i, $END) as j
    ON CONFLICT (account_id) DO NOTHING;"
done

echo -e "${GREEN}Account generation completed!${NC}"

# Generate account sessions
echo -e "${YELLOW}Generating account sessions...${NC}"

for ((i=1; i<=TOTAL_ACCOUNTS; i+=BATCH_SIZE)); do
    END=$((i+BATCH_SIZE-1))
    if [ $END -gt $TOTAL_ACCOUNTS ]; then
        END=$TOTAL_ACCOUNTS
    fi
    
    echo -e "${GREEN}Generating sessions $i to $END...${NC}"
    
    # account_session의 id는 32자 제한이 있으므로 조심해서 생성
    execute_sql "
    INSERT INTO account_session (id, account_id)
    SELECT 
        LPAD(j::text, 32, '0') as id,
        '0xAccount' || LPAD(j::text, 33, '0') as account_id
    FROM generate_series($i, $END) as j
    ON CONFLICT (account_id) DO NOTHING;"
done

echo -e "${GREEN}Account session generation completed!${NC}"

# Generate account_x
echo -e "${YELLOW}Generating account X profiles...${NC}"

for ((i=1; i<=TOTAL_ACCOUNTS; i+=BATCH_SIZE)); do
    END=$((i+BATCH_SIZE-1))
    if [ $END -gt $TOTAL_ACCOUNTS ]; then
        END=$TOTAL_ACCOUNTS
    fi
    
    echo -e "${GREEN}Generating X profiles $i to $END...${NC}"
    
    execute_sql "
    INSERT INTO account_x (account_id, x_handle, x_image_uri, is_blue_label)
    SELECT 
        '0xAccount' || LPAD(j::text, 33, '0') as account_id,
        LPAD(j::text, 15, '0') as x_handle,
        'a' as x_image_uri,
        true as is_blue_label
    FROM generate_series($i, $END) as j
    ON CONFLICT (x_handle) DO NOTHING;"
done

echo -e "${GREEN}Account X profile generation completed!${NC}"

# Generate tokens
echo -e "${YELLOW}Generating tokens (10,000 per account for first 1,000 accounts)...${NC}"

# First 1000 accounts will create 10,000 tokens each = 10 million tokens total
ACCOUNTS_WITH_TOKENS=1000
TOKENS_PER_ACCOUNT=10000
TOKEN_BATCH_SIZE=100000

TOKEN_ID=1
for ((ACCOUNT_ID=1; ACCOUNT_ID<=ACCOUNTS_WITH_TOKENS; ACCOUNT_ID++)); do
    echo -e "${GREEN}Generating tokens for account $ACCOUNT_ID...${NC}"
    
    # Generate tokens in batches for this account
    for ((j=1; j<=TOKENS_PER_ACCOUNT; j+=TOKEN_BATCH_SIZE)); do
        BATCH_END=$((j+TOKEN_BATCH_SIZE-1))
        if [ $BATCH_END -gt $TOKENS_PER_ACCOUNT ]; then
            BATCH_END=$TOKENS_PER_ACCOUNT
        fi
        
        BATCH_SIZE_ACTUAL=$((BATCH_END-j+1))
        
        execute_sql "
        INSERT INTO token (token_id, name, symbol, image_uri, creator, description, twitter, telegram, website, is_listing, created_at, transaction_hash, total_supply)
        SELECT 
            '0xToken' || LPAD((($TOKEN_ID + row_number() OVER () - 1))::text, 35, '0') as token_id,
            'Token' || ($TOKEN_ID + row_number() OVER () - 1) as name,
            'TKN' || ($TOKEN_ID + row_number() OVER () - 1) as symbol,
            'https://example.com/token.png' as image_uri,
            '0xAccount' || LPAD($ACCOUNT_ID::text, 33, '0') as creator,
            'Token description' as description,
            'https://twitter.com/token' as twitter,
            'https://t.me/token' as telegram,
            'https://token.com' as website,
            false as is_listing,
            -- 더 현실적인 created_at: 지난 1년간 랜덤하게 분포
            1700000000 + (($TOKEN_ID + row_number() OVER () - 1) * 3 + floor(random() * 86400))::bigint as created_at,
            '0x' || md5(random()::text || 'tx')::varchar(64) as transaction_hash,
            1000000000 as total_supply
        FROM generate_series(1, $BATCH_SIZE_ACTUAL)
        ON CONFLICT (token_id) DO NOTHING;"
        
        TOKEN_ID=$((TOKEN_ID + BATCH_SIZE_ACTUAL))
    done
done

echo -e "${GREEN}Token generation completed!${NC}"

# Update token_count
echo -e "${YELLOW}Updating token count...${NC}"
execute_sql "
INSERT INTO token_count (total_count, listing_count)
VALUES (0, 0)
ON CONFLICT DO NOTHING;

UPDATE token_count SET 
    total_count = (SELECT COUNT(*) FROM token),
    listing_count = (SELECT COUNT(*) FROM token WHERE is_listing = true);"

# Generate king entries - 더 현실적으로: 시간별로 하나씩만 킹이 됨
echo -e "${YELLOW}Generating king entries (realistic: one king per hour)...${NC}"

execute_sql "
-- 킹 토큰을 더 현실적으로 생성 (시간당 1개씩, 랜덤 토큰이 킹이 됨)
WITH king_times AS (
    SELECT 
        1700000000 + (i * 3600)::bigint as king_time,  -- 매 시간마다
        floor(random() * 10000000 + 1)::int as random_token_num
    FROM generate_series(1, 8760) as i  -- 1년치 (365일 * 24시간)
)
INSERT INTO king (token_id, transaction_hash, created_at)
SELECT 
    '0xToken' || LPAD(random_token_num::text, 35, '0') as token_id,
    '0x' || md5(random()::text || 'king' || random_token_num::text || king_time::text)::varchar(64) as transaction_hash,
    king_time as created_at
FROM king_times
WHERE random_token_num <= 10000000  -- 실제 존재하는 토큰만
ON CONFLICT (token_id) DO NOTHING;"

echo -e "${GREEN}King entries generation completed!${NC}"

# Generate markets for all tokens
echo -e "${YELLOW}Generating markets for all tokens...${NC}"

# Generate markets in batches
MARKET_BATCH_SIZE=100000
TOTAL_TOKENS=10000000

for ((i=1; i<=TOTAL_TOKENS; i+=MARKET_BATCH_SIZE)); do
    END=$((i+MARKET_BATCH_SIZE-1))
    if [ $END -gt $TOTAL_TOKENS ]; then
        END=$TOTAL_TOKENS
    fi
    
    echo -e "${GREEN}Generating markets for tokens $i to $END...${NC}"
    
    execute_sql "
    INSERT INTO market (market_type, token_id, pool_id, reserve_token, price, latest_trade_at, created_at)
    SELECT 
        CASE WHEN j % 2 = 1 THEN 'CURVE' ELSE 'DEX' END as market_type,
        '0xToken' || LPAD(j::text, 35, '0') as token_id,
        CASE WHEN j % 2 = 0 THEN '0xPool' || LPAD(j::text, 36, '0') ELSE NULL END as pool_id,
        CASE WHEN j % 2 = 1 THEN 1000000::numeric ELSE NULL END as reserve_token,
        -- 더 다양한 price 분포 (0.00001 ~ 1000)
        CASE 
            WHEN j % 100 = 0 THEN (10 + random() * 990)::numeric(15,10)  -- 1% 고가
            WHEN j % 20 = 0 THEN (1 + random() * 9)::numeric(15,10)      -- 5% 중고가
            WHEN j % 5 = 0 THEN (0.1 + random() * 0.9)::numeric(15,10)   -- 20% 중가
            ELSE (0.00001 + random() * 0.09999)::numeric(15,10)          -- 74% 저가
        END as price,
        -- latest_trade_at: created_at 이후 랜덤하게 분포 (더 현실적)
        1700000000 + (j * 3) + floor(random() * 86400 * 30)::bigint as latest_trade_at,
        1700000000 + (j * 3)::bigint as created_at
    FROM generate_series($i, $END) as j
    ON CONFLICT (token_id) DO NOTHING;"
done

echo -e "${GREEN}Market generation completed!${NC}"

# Generate balances
echo -e "${YELLOW}Generating balances...${NC}"

BALANCE_BATCH_SIZE=100000

# 1. Generate 1M balances for token 1 with various accounts and different balance amounts
echo -e "${GREEN}Generating 1,000,000 balances for token 1...${NC}"

BALANCES_FOR_TOKEN1=1000000

for ((i=1; i<=BALANCES_FOR_TOKEN1; i+=BALANCE_BATCH_SIZE)); do
    END=$((i+BALANCE_BATCH_SIZE-1))
    if [ $END -gt $BALANCES_FOR_TOKEN1 ]; then
        END=$BALANCES_FOR_TOKEN1
    fi
    
    echo -e "${GREEN}Generating balances $i to $END for token 1...${NC}"
    
    execute_sql "
    INSERT INTO balance (account_id, token_id, balance)
    SELECT 
        '0xAccount' || LPAD(j::text, 33, '0') as account_id,
        '0xToken' || LPAD('1', 35, '0') as token_id,
        CASE 
            WHEN j % 10 = 0 THEN (1000000000000000000::numeric * (1 + (j % 100)))  -- 10% have large balances
            WHEN j % 3 = 0 THEN (1000000000000000::numeric * (1 + (j % 50)))       -- 30% have medium balances
            ELSE (1000000000000::numeric * (1 + (j % 20)))                         -- 60% have small balances
        END as balance
    FROM generate_series($i, $END) as j
    ON CONFLICT (account_id, token_id) DO NOTHING;"
done

# 2. Generate 10K balances for account 1 with various tokens (all >= 1e18)
echo -e "${GREEN}Generating 10,000 balances for account 1 with balance >= 1e18...${NC}"

TOKENS_FOR_ACCOUNT1=10000

execute_sql "
INSERT INTO balance (account_id, token_id, balance)
SELECT 
    '0xAccount' || LPAD('1', 33, '0') as account_id,
    '0xToken' || LPAD((j + 1)::text, 35, '0') as token_id,  -- Tokens 2 to 10001
    (1000000000000000000::numeric * (1 + (j % 1000))) as balance  -- All >= 1e18, varying amounts
FROM generate_series(1, $TOKENS_FOR_ACCOUNT1) as j
ON CONFLICT (account_id, token_id) DO NOTHING;"

echo -e "${GREEN}Balance generation completed!${NC}"

# Generate swaps
echo -e "${YELLOW}Generating swaps...${NC}"

# 1. Generate 1M swaps for token 1 with various accounts
echo -e "${GREEN}Generating 1,000,000 swaps for token 1...${NC}"

SWAPS_FOR_TOKEN1=1000000
SWAP_BATCH_SIZE=100000
SWAP_ACCOUNTS=10000  # Use 10,000 different accounts

for ((i=1; i<=SWAPS_FOR_TOKEN1; i+=SWAP_BATCH_SIZE)); do
    END=$((i+SWAP_BATCH_SIZE-1))
    if [ $END -gt $SWAPS_FOR_TOKEN1 ]; then
        END=$SWAPS_FOR_TOKEN1
    fi
    
    echo -e "${GREEN}Generating swaps $i to $END for token 1...${NC}"
    
    execute_sql "
    INSERT INTO swap (account_id, token_id, market_type, is_buy, native_amount, token_amount, is_dev, created_at, transaction_hash, log_index)
    SELECT 
        '0xAccount' || LPAD(((j-1) % $SWAP_ACCOUNTS + 1)::text, 33, '0') as account_id,
        '0xToken' || LPAD('1', 35, '0') as token_id,
        'CURVE' as market_type,
        (j % 2 = 0) as is_buy,
        (1 + (j % 100))::numeric as native_amount,
        (1000 + (j % 10000))::numeric as token_amount,
        false as is_dev,
        -- swap 시간을 market의 latest_trade_at과 연관되게 설정
        1700000000 + (j * 10) + floor(random() * 86400)::bigint as created_at,
        '0x' || md5(random()::text || 'swap_token1_' || j)::varchar(64) as transaction_hash,
        (j % 10)::int as log_index
    FROM generate_series($i, $END) as j
    ON CONFLICT (account_id, token_id, transaction_hash, log_index) DO NOTHING;"
done

# 2. Generate 1M swaps for account 1 with various tokens
echo -e "${GREEN}Generating 1,000,000 swaps for account 1...${NC}"

SWAPS_FOR_ACCOUNT1=1000000
TOKENS_FOR_ACCOUNT1=10000  # Account 1 will trade 10,000 different tokens

for ((i=1; i<=SWAPS_FOR_ACCOUNT1; i+=SWAP_BATCH_SIZE)); do
    END=$((i+SWAP_BATCH_SIZE-1))
    if [ $END -gt $SWAPS_FOR_ACCOUNT1 ]; then
        END=$SWAPS_FOR_ACCOUNT1
    fi
    
    echo -e "${GREEN}Generating swaps $i to $END for account 1...${NC}"
    
    execute_sql "
    INSERT INTO swap (account_id, token_id, market_type, is_buy, native_amount, token_amount, is_dev, created_at, transaction_hash, log_index)
    SELECT 
        '0xAccount' || LPAD('1', 33, '0') as account_id,
        '0xToken' || LPAD(((j-1) % $TOKENS_FOR_ACCOUNT1 + 2)::text, 35, '0') as token_id,  -- Start from token 2 to avoid duplicates
        CASE WHEN ((j-1) % $TOKENS_FOR_ACCOUNT1 + 2) % 2 = 1 THEN 'CURVE' ELSE 'DEX' END as market_type,
        (j % 2 = 0) as is_buy,
        (1 + (j % 100))::numeric as native_amount,
        (1000 + (j % 10000))::numeric as token_amount,
        false as is_dev,
        -- swap 시간을 market의 latest_trade_at과 연관되게 설정
        1700000000 + (j * 10) + floor(random() * 86400)::bigint as created_at,
        '0x' || md5(random()::text || 'swap_account1_' || j)::varchar(64) as transaction_hash,
        (j % 10)::int as log_index
    FROM generate_series($i, $END) as j
    ON CONFLICT (account_id, token_id, transaction_hash, log_index) DO NOTHING;"
done

echo -e "${GREEN}Swap generation completed!${NC}"

# Generate charts for token 1
echo -e "${YELLOW}Generating 100,000 chart entries for token 1 with interval_type 1...${NC}"

CHART_ENTRIES=100000
CHART_BATCH_SIZE=10000
BASE_TIMESTAMP=$(date +%s)
BASE_PRICE=10.0

for ((i=1; i<=CHART_ENTRIES; i+=CHART_BATCH_SIZE)); do
    END=$((i+CHART_BATCH_SIZE-1))
    if [ $END -gt $CHART_ENTRIES ]; then
        END=$CHART_ENTRIES
    fi
    
    echo -e "${GREEN}Generating chart entries $i to $END...${NC}"
    
    execute_sql "
    INSERT INTO chart (token_id, interval_type, open_price, close_price, high_price, low_price, volume, time_stamp)
    SELECT 
        '0xToken' || LPAD('1', 35, '0') as token_id,
        '1' as interval_type,
        ($BASE_PRICE + sin(j::float / 1000) * 2)::numeric(15,10) as open_price,
        ($BASE_PRICE + sin((j+1)::float / 1000) * 2)::numeric(15,10) as close_price,
        ($BASE_PRICE + sin(j::float / 1000) * 2 + abs(random()) * 0.5)::numeric(15,10) as high_price,
        ($BASE_PRICE + sin(j::float / 1000) * 2 - abs(random()) * 0.5)::numeric(15,10) as low_price,
        (100000 + floor(random() * 900000))::numeric as volume,
        ($BASE_TIMESTAMP::bigint + ((j-1)::bigint * 60))::bigint as time_stamp
    FROM generate_series($i, $END) as j
    ON CONFLICT (token_id, interval_type, time_stamp) DO NOTHING;"
done

echo -e "${GREEN}Chart generation completed!${NC}"

# Generate price entries
echo -e "${YELLOW}Generating 10 million price entries...${NC}"

PRICE_ENTRIES=10000000
PRICE_BATCH_SIZE=100000

for ((i=1; i<=PRICE_ENTRIES; i+=PRICE_BATCH_SIZE)); do
    END=$((i+PRICE_BATCH_SIZE-1))
    if [ $END -gt $PRICE_ENTRIES ]; then
        END=$PRICE_ENTRIES
    fi
    
    echo -e "${GREEN}Generating price entries $i to $END...${NC}"
    
    execute_sql "
    INSERT INTO price (block_number, price, created_at)
    SELECT 
        j::bigint as block_number,
        1::numeric as price,
        j::bigint as created_at
    FROM generate_series($i, $END) as j
    ON CONFLICT (block_number) DO NOTHING;"
done

echo -e "${GREEN}Price generation completed!${NC}"

# Generate burn history
echo -e "${YELLOW}Generating burn history for tokens 1-1000...${NC}"

BURN_TOKENS=1000
BURNS_PER_TOKEN=100
BURN_ACCOUNT_ID=1

for ((TOKEN_ID=1; TOKEN_ID<=BURN_TOKENS; TOKEN_ID++)); do
    echo -e "${GREEN}Generating burn history for token $TOKEN_ID...${NC}"
    
    execute_sql "
    INSERT INTO burn_history (token_id, account_id, token_amount, transaction_hash, created_at, log_index)
    SELECT 
        '0xToken' || LPAD($TOKEN_ID::text, 35, '0') as token_id,
        '0xAccount' || LPAD($BURN_ACCOUNT_ID::text, 33, '0') as account_id,
        (100 + (i % 1000))::numeric as token_amount,
        '0x' || md5(random()::text || 'burn' || $TOKEN_ID || '_' || i)::varchar(64) as transaction_hash,
        extract(epoch from now() - interval '1 day' * (i % 30))::bigint as created_at,
        (i % 10)::int as log_index
    FROM generate_series(1, $BURNS_PER_TOKEN) as i
    ON CONFLICT (token_id, account_id, transaction_hash, log_index) DO NOTHING;"
done

echo -e "${GREEN}Burn history generation completed!${NC}"

# Generate follow relationships
echo -e "${YELLOW}Generating follow relationships...${NC}"

# Account 1 follows accounts 2-10000
echo -e "${GREEN}Account 1 following accounts 2-10000...${NC}"
execute_sql "
INSERT INTO follow (follower_id, following_id)
SELECT 
    '0xAccount' || LPAD('1', 33, '0') as follower_id,
    '0xAccount' || LPAD(i::text, 33, '0') as following_id
FROM generate_series(2, 10000) as i
ON CONFLICT (follower_id, following_id) DO NOTHING;"

# Accounts 2-10000 follow account 1
echo -e "${GREEN}Accounts 2-10000 following account 1...${NC}"
execute_sql "
INSERT INTO follow (follower_id, following_id)
SELECT 
    '0xAccount' || LPAD(i::text, 33, '0') as follower_id,
    '0xAccount' || LPAD('1', 33, '0') as following_id
FROM generate_series(2, 10000) as i
ON CONFLICT (follower_id, following_id) DO NOTHING;"

echo -e "${GREEN}Follow relationships generation completed!${NC}"

# Update follower/following counts for affected accounts
echo -e "${YELLOW}Updating follower/following counts...${NC}"
execute_sql "
UPDATE account SET 
    follower_count = 9999,
    following_count = 9999
WHERE account_id = '0xAccount' || LPAD('1', 33, '0');

UPDATE account SET 
    follower_count = 1,
    following_count = 1
WHERE account_id IN (
    SELECT '0xAccount' || LPAD(i::text, 33, '0')
    FROM generate_series(2, 10000) as i
);"

# Show statistics
echo -e "${YELLOW}Database statistics:${NC}"
execute_sql "
SELECT 
    'Accounts' as table_name, 
    COUNT(*) as count 
FROM account
UNION ALL
SELECT 
    'Account Sessions' as table_name,
    COUNT(*) as count
FROM account_session
UNION ALL
SELECT 
    'Account X Profiles' as table_name,
    COUNT(*) as count
FROM account_x
UNION ALL
SELECT 
    'Tokens' as table_name,
    COUNT(*) as count
FROM token
UNION ALL
SELECT 
    'King Tokens' as table_name,
    COUNT(*) as count
FROM king
UNION ALL
SELECT 
    'Markets' as table_name,
    COUNT(*) as count
FROM market
UNION ALL
SELECT 
    'Balances' as table_name,
    COUNT(*) as count
FROM balance
UNION ALL
SELECT 
    'Swaps' as table_name,
    COUNT(*) as count
FROM swap
UNION ALL
SELECT 
    'Charts' as table_name,
    COUNT(*) as count
FROM chart
UNION ALL
SELECT 
    'Prices' as table_name,
    COUNT(*) as count
FROM price
UNION ALL
SELECT 
    'Burn History' as table_name,
    COUNT(*) as count
FROM burn_history
UNION ALL
SELECT 
    'Follows' as table_name,
    COUNT(*) as count
FROM follow;"

# 데이터 분포 확인
echo -e "${YELLOW}Data distribution check:${NC}"
execute_sql "
-- Token created_at 분포 확인
SELECT 
    'Token created_at' as metric,
    COUNT(DISTINCT created_at) as unique_values,
    MIN(created_at) as min_value,
    MAX(created_at) as max_value
FROM token
UNION ALL
-- Market latest_trade_at 분포 확인
SELECT 
    'Market latest_trade_at' as metric,
    COUNT(DISTINCT latest_trade_at) as unique_values,
    MIN(latest_trade_at) as min_value,
    MAX(latest_trade_at) as max_value
FROM market
UNION ALL
-- King created_at 분포 확인
SELECT 
    'King created_at' as metric,
    COUNT(DISTINCT created_at) as unique_values,
    MIN(created_at) as min_value,
    MAX(created_at) as max_value
FROM king
UNION ALL
-- Price 분포 확인
SELECT 
    'Market price range' as metric,
    COUNT(DISTINCT price) as unique_values,
    MIN(price)::bigint as min_value,
    MAX(price)::bigint as max_value
FROM market;"

echo -e "${GREEN}Script completed!${NC}"