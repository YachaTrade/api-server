# Database Queries in Types Directory

This document contains all SQL queries found in the `/src/types` directory, organized by controller.

## Table of Contents
1. [Account Controller](#account-controller)
2. [Account Wallet Controller](#account-wallet-controller)
3. [Account X Controller](#account-x-controller)
4. [Auth Session Controller](#auth-session-controller)
5. [Follow Controller](#follow-controller)
6. [Search Controller](#search-controller)
7. [Token Controller](#token-controller)
8. [Token Created Controller](#token-created-controller)
9. [Token Metadata Controller](#token-metadata-controller)
10. [Hype Token Controller](#hype-token-controller)
11. [Order Controller](#order-controller)
12. [Trading Swap History Controller](#trading-swap-history-controller)
13. [Trading Position Controller](#trading-position-controller)
14. [Trading Price Controller](#trading-price-controller)
15. [Trading Market Controller](#trading-market-controller)
16. [Trading Chart Controller](#trading-chart-controller)
17. [New Content Controller](#new-content-controller)
18. [Token Management Controller](#token-management-controller)

---

## Account Controller
**File**: `/src/types/account/mod.rs`

### 1. Insert/Update Account
```sql
INSERT INTO account (account_id, image_uri, nickname, bio, follower_count, following_count)
VALUES ($1, $2, $3, $4, $5, $6)
ON CONFLICT (account_id) 
DO UPDATE SET
    image_uri = COALESCE(EXCLUDED.image_uri, account.image_uri),
    nickname = COALESCE(EXCLUDED.nickname, account.nickname),
    bio = COALESCE(EXCLUDED.bio, account.bio),
    follower_count = GREATEST(account.follower_count, EXCLUDED.follower_count),
    following_count = GREATEST(account.following_count, EXCLUDED.following_count)
RETURNING *
```

### 2. Update Account (Dynamic)
```sql
UPDATE account SET 
[dynamic fields based on request]
WHERE account_id = $X
RETURNING *
```

### 3. Get Account
```sql
SELECT a.account_id,
a.nickname,
a.image_uri,
a.bio,
a.follower_count,
a.following_count,
x.x_handle,
x.x_image_uri,
x.is_blue_label
FROM account a
LEFT JOIN account_x x ON a.account_id = x.account_id
WHERE a.account_id = $1
```

### 4. Get Account with Mutual Friends
```sql
WITH target_account AS (
    SELECT 
        a.account_id,
        a.nickname,
        a.image_uri,
        a.bio,
        a.follower_count,
        a.following_count,
        x.x_handle,
        x.x_image_uri,
        x.is_blue_label
    FROM account a
    LEFT JOIN account_x x ON a.account_id = x.account_id
    WHERE a.account_id = $1
),
mutual_friends AS (
    SELECT 
        a.account_id,
        a.nickname,
        a.image_uri,
        a.follower_count,
        a.following_count
    FROM follow f1
    JOIN follow f2 ON f1.following_id = f2.following_id
    JOIN account a ON f1.following_id = a.account_id
    WHERE f1.follower_id = $2 
    AND f2.follower_id = $1
    AND f1.following_id != $1 
    AND f1.following_id != $2
    ORDER BY a.follower_count DESC
    LIMIT 3
)
SELECT 
    ta.account_id,
    ta.nickname,
    ta.image_uri,
    ta.bio,
    ta.follower_count,
    ta.following_count,
    ta.x_handle,
    ta.x_image_uri,
    ta.is_blue_label,
    COALESCE(json_agg(
        json_build_object(
            'account_id', mf.account_id,
            'nickname', mf.nickname,
            'image_uri', mf.image_uri,
            'follower_count', mf.follower_count,
            'following_count', mf.following_count
        )
    ) FILTER (WHERE mf.account_id IS NOT NULL), '[]'::json) as mutual_follow
FROM target_account ta
LEFT JOIN mutual_friends mf ON true
GROUP BY 
    ta.account_id, ta.nickname, ta.image_uri, ta.bio, 
    ta.follower_count, ta.following_count, ta.x_handle,
    ta.x_image_uri, ta.is_blue_label
```

---

## Account Wallet Controller
**File**: `/src/types/account/wallet.rs`

### 1. Insert/Update Account Wallet
```sql
INSERT INTO account_wallet (account_id, wallet)
VALUES ($1, $2)
ON CONFLICT (account_id) DO UPDATE
SET wallet = EXCLUDED.wallet
RETURNING account_id, wallet
```

### 2. Get Account Wallet
```sql
SELECT account_id, wallet
FROM account_wallet
WHERE account_id = $1
```

---

## Account X Controller
**File**: `/src/types/account/x.rs`

### 1. Insert Account X
```sql
INSERT INTO account_x (account_id, x_handle, x_image_uri, is_blue_label)
VALUES ($1, $2, $3, $4)
RETURNING *
```

### 2. Check Account X Exists
```sql
SELECT 1 as exists FROM account_x WHERE account_id = $1 AND x_handle = $2
```

### 3. Delete Account X
```sql
DELETE FROM account_x
WHERE account_id = $1 AND x_handle = $2
```

### 4. Get Account X
```sql
SELECT x_handle, x_image_uri, is_blue_label
FROM account_x
WHERE account_id = $1
```

---

## Auth Session Controller
**File**: `/src/types/auth/mod.rs`

### 1. Set Session
```sql
INSERT INTO account_session (id, account_id)
VALUES ($1, $2)
ON CONFLICT (account_id) DO UPDATE
SET id = EXCLUDED.id
```

### 2. Get Address by Session ID
```sql
SELECT account_id FROM account_session WHERE id = $1
```

### 3. Delete Session by Address
```sql
DELETE FROM account_session WHERE account_id = $1
```

### 4. Delete Session by ID
```sql
DELETE FROM account_session WHERE id = $1
```

---

## Follow Controller
**File**: `/src/types/social/follow.rs`

### 1. Get Follows (Followers/Following)
```sql
SELECT 
    a.account_id,
    a.nickname,
    a.image_uri,
    a.follower_count,
    a.following_count
FROM follow f
JOIN account a ON CASE 
    WHEN $2 = true THEN a.account_id = f.following_id  -- Get following
    ELSE a.account_id = f.follower_id                  -- Get followers
END
WHERE CASE 
    WHEN $2 = true THEN f.follower_id = $1   -- account_id is following others
    ELSE f.following_id = $1                 -- others are following account_id
END
ORDER BY a.follower_count DESC
LIMIT $3
OFFSET $4
```

### 2. Insert Follow
```sql
INSERT INTO follow (follower_id, following_id)
VALUES ($1, $2)
ON CONFLICT DO NOTHING
```

### 3. Update Account Following Count (Add)
```sql
UPDATE account
SET following_count = following_count + 1
WHERE account_id = $1
RETURNING account_id, nickname, image_uri, follower_count, following_count
```

### 4. Update Account Follower Count (Add)
```sql
UPDATE account
SET follower_count = follower_count + 1
WHERE account_id = $1
RETURNING account_id, nickname, image_uri, follower_count, following_count
```

### 5. Update Account Following Count (Remove)
```sql
UPDATE account
SET following_count = GREATEST(following_count - 1, 0)
WHERE account_id = $1
RETURNING account_id, nickname, image_uri, follower_count, following_count
```

### 6. Update Account Follower Count (Remove)
```sql
UPDATE account
SET follower_count = GREATEST(follower_count - 1, 0)
WHERE account_id = $1
RETURNING account_id, nickname, image_uri, follower_count, following_count
```

### 7. Delete Follow
```sql
DELETE FROM follow
WHERE follower_id = $1 AND following_id = $2
```

### 8. Check Follow Exists
```sql
SELECT EXISTS (
    SELECT 1 FROM follow 
    WHERE follower_id = $1 AND following_id = $2
) as exists
```

---

## Search Controller
**File**: `/src/types/search/mod.rs`

### 1. Search Tokens
```sql
SELECT 
    t.token_id, t.name, t.symbol, t.image_uri,
    t.created_at, t.total_supply, m.market_type, m.price
FROM token t
JOIN market m ON t.token_id = m.token_id

WHERE 
   -- 정확한 매칭 (최우선, 가장 빠름)
    LOWER(t.token_id) = LOWER($1)
    OR LOWER(t.name) = LOWER($1)
    OR LOWER(t.symbol) = LOWER($1)
    -- Trigram 유사도 매칭 (느리지만 유연함)
    OR LOWER(t.token_id) % LOWER($1)
    OR LOWER(t.name) % LOWER($1)
    OR LOWER(t.symbol) % LOWER($1)
ORDER BY 
    CASE 
        WHEN LOWER(t.name) = LOWER($1) OR LOWER(t.symbol) = LOWER($1) THEN 0
        ELSE 1
    END,
    m.price DESC
LIMIT 50
```

### 2. Search Accounts
```sql
SELECT a.account_id, nickname, image_uri, follower_count, following_count,
ax.x_handle,
ax.x_image_uri,
ax.is_blue_label
FROM account a
LEFT JOIN account_x ax ON a.account_id = ax.account_id
WHERE 
    LOWER(a.nickname) = LOWER($1)
    OR LOWER(a.account_id) = LOWER($1)
    OR LOWER(a.nickname) % LOWER($1)
    OR LOWER(a.account_id) % LOWER($1)
ORDER BY 
    CASE 
        WHEN LOWER(a.nickname) = LOWER($1) OR LOWER(a.account_id) = LOWER($1) THEN 0
        ELSE 1
    END,
    follower_count DESC
LIMIT 5
```

---

## Token Controller
**File**: `/src/types/token/mod.rs`

### 1. Get Token Details
```sql
SELECT 
    t.token_id,
    t.name,
    t.symbol,
    t.description,
    t.twitter,
    t.telegram,
    t.website,
    t.image_uri,
    t.is_listing,
    t.total_supply,
    m.price,
    t.created_at,
    t.transaction_hash,
    COALESCE(k.token_id IS NOT NULL, false)::boolean as is_king,
    k.created_at as is_king_created_at,
    t.creator,
    a.nickname as creator_nickname,
    a.image_uri as creator_image_uri,
    a.follower_count as creator_follower_count, 
    a.following_count as creator_following_count,
    ax.x_handle,
    ax.x_image_uri
FROM token t
LEFT JOIN king k ON t.token_id = k.token_id
LEFT JOIN account_x ax ON t.creator = ax.account_id
JOIN market m ON t.token_id = m.token_id
JOIN account a ON t.creator = a.account_id
WHERE t.token_id = $1
```

---

## Token Created Controller
**File**: `/src/types/token/create_token.rs`

### 1. Get Token Created Count
```sql
SELECT COALESCE(COUNT(*)::bigint, 0) as count
FROM token t
WHERE t.creator = $1
```

### 2. Get Tokens Created by Account
```sql
WITH created_tokens AS (
    SELECT 
        t.token_id,
        t.symbol,
        t.image_uri,
        t.name,
        t.total_supply,
        t.description,
        t.created_at,
        t.creator,
        t.is_listing,
        COALESCE(m.price, 0) as price,
        COALESCE(b.balance, 0) as current_amount,
        COALESCE(m.price * b.balance, 0) as current_value
    FROM token t
    LEFT JOIN market m ON t.token_id = m.token_id
    LEFT JOIN balance b ON t.token_id = b.token_id AND b.account_id = $1
    WHERE t.creator = $1
)
SELECT 
    token_id,
    symbol,
    image_uri,
    name,
    is_listing as "is_listing!",
    created_at,
    COALESCE(price::TEXT, '0') as "price!",
    total_supply as "total_supply!",
    COALESCE(price * total_supply, 0) as "market_cap!",
    COALESCE(current_amount, 0) as "current_amount!",
    description as "description?: String"
FROM created_tokens
ORDER BY current_value DESC
LIMIT $2
OFFSET $3
```

---

## Token Metadata Controller
**File**: `/src/types/token/metadata.rs`

### 1. Get Token Metadata
```sql
SELECT token_id, name, symbol, image_uri FROM token WHERE token_id = $1
```

---

## Hype Token Controller
**File**: `/src/types/token/hype.rs`

### 1. Get Hype Tokens
```sql
SELECT 
    h.token_id,
    t.name,
    t.symbol,
    t.image_uri,
    t.description,
    t.creator as creator_account_id,
    a.nickname as creator_nickname,
    a.image_uri as creator_image_uri,
    a.follower_count as creator_follower_count, 
    a.following_count as creator_following_count,
    x.x_handle as x_handle,
    x.x_image_uri as x_image_uri,
    x.is_blue_label as is_blue_label,
    -- 홀더 수 계산 - 기존 인덱스 활용 (idx_position_token_is_active)
    (SELECT COUNT(*) FROM balance b WHERE b.token_id = h.token_id  AND b.balance > 0) as holder_count,
    -- 시가총액 계산 (가격 * 총 공급량)
    COALESCE(m.price * t.total_supply, 0) as market_cap,
    -- 현재 가격
    m.price as current_price,
    -- 24시간 전 가격 (24시간 전 데이터가 없으면 가장 오래된 데이터 사용)
    COALESCE(
        (SELECT c.close_price 
         FROM chart c 
         WHERE c.token_id = h.token_id 
           AND c.interval_type = $1 
           AND c.time_stamp <= $2 
         ORDER BY c.time_stamp DESC 
         LIMIT 1),
        (SELECT c.close_price 
         FROM chart c 
         WHERE c.token_id = h.token_id 
           AND c.interval_type = $1 
         ORDER BY c.time_stamp ASC 
         LIMIT 1)
    ) as day_ago_price
FROM hype_token h
-- 필요한 테이블만 먼저 조인 (최소 필수 조인 먼저 수행)
JOIN token t ON h.token_id = t.token_id
JOIN market m ON h.token_id = m.token_id
JOIN account a ON t.creator = a.account_id
LEFT JOIN account_x x ON a.account_id = x.account_id
ORDER BY m.price DESC NULLS LAST
LIMIT $3 OFFSET $4
```

### 2. Get Hype Token Count
```sql
SELECT COUNT(*) as count
FROM hype_token h
```

---

## Order Controller
**File**: `/src/types/token/order.rs`

### 1. Get Tokens by Creation Time
```sql
SELECT 
    t.token_id, a.account_id, a.follower_count, a.following_count, 
    a.nickname, a.image_uri as account_image_uri, t.name, t.symbol, 
    t.image_uri as token_image_uri, t.description, 
    t.total_supply as total_supply,
    COALESCE(m.price, '0') as price,
    COALESCE(m.reserve_token, '0') as reserve_token,
    ax.x_handle,
    ax.x_image_uri,
    ax.is_blue_label,
    m.market_type, t.created_at, t.created_at::FLOAT8 as score
FROM token t
JOIN account a ON t.creator = a.account_id
LEFT JOIN account_x ax ON t.creator = ax.account_id
LEFT JOIN market m ON t.token_id = m.token_id
ORDER BY t.created_at [DESC/ASC]
LIMIT $1 OFFSET $2
```

### 2. Get Tokens by Latest Trade
```sql
SELECT 
    t.token_id, a.account_id, a.nickname, a.image_uri as account_image_uri,
    a.follower_count, a.following_count, t.name, t.symbol,
    t.image_uri as token_image_uri, t.description,
    t.total_supply as total_supply,
    COALESCE(m.price, '0') as price,
    COALESCE(m.reserve_token, '0') as reserve_token,
    ax.x_handle,
    ax.x_image_uri,
    ax.is_blue_label,
    m.market_type, t.created_at, m.latest_trade_at::FLOAT8 as score
FROM market m
JOIN token t ON m.token_id = t.token_id
JOIN account a ON t.creator = a.account_id
LEFT JOIN account_x ax ON t.creator = ax.account_id
ORDER BY m.latest_trade_at [DESC/ASC]
LIMIT $1 OFFSET $2
```

### 3. Get Tokens by Market Cap
```sql
SELECT 
    t.token_id, a.account_id, a.nickname, a.image_uri as account_image_uri,
    a.follower_count, a.following_count, t.name, t.symbol,
    t.image_uri as token_image_uri, t.description,
    t.total_supply as total_supply,
    COALESCE(m.price, '0') as price,
    COALESCE(m.reserve_token, '0') as reserve_token,
    ax.x_handle,
    ax.x_image_uri,
    ax.is_blue_label,
    m.market_type, t.created_at, m.price::FLOAT8 as score
FROM (
    SELECT token_id, price, reserve_token, market_type
    FROM market
    ORDER BY price [DESC/ASC]
    LIMIT $1 OFFSET $2
) m
JOIN token t ON m.token_id = t.token_id
JOIN account a ON t.creator = a.account_id
LEFT JOIN account_x ax ON t.creator = ax.account_id
ORDER BY m.price [DESC/ASC]
```

### 4. Get Latest King of the Hill
```sql
SELECT 
    t.token_id,
    a.account_id,
    a.nickname,
    a.image_uri as account_image_uri,
    a.follower_count,
    a.following_count,
    t.name,
    t.symbol,
    t.image_uri as token_image_uri,
    t.description,
    t.total_supply as total_supply,
    COALESCE(m.price, '0') as price,
    COALESCE(m.reserve_token, '0') as reserve_token,
    m.market_type,
    t.created_at,
    k.created_at::FLOAT8 as score,
    ax.x_handle,
    ax.x_image_uri,
    ax.is_blue_label
FROM king k
JOIN token t ON k.token_id = t.token_id
JOIN account a ON t.creator = a.account_id
LEFT JOIN account_x ax ON t.creator = ax.account_id
LEFT JOIN market m ON t.token_id = m.token_id
ORDER BY k.created_at DESC
LIMIT 1
```

---

## Trading Swap History Controller
**File**: `/src/types/trading/swap_history.rs`

### 1. Get Account Swap Count
```sql
SELECT COALESCE(COUNT(*)::bigint, 0) as count
FROM swap s
WHERE s.account_id = $1
```

### 2. Get Account Swaps
```sql
SELECT 
    s.account_id,
    s.token_id,
    s.transaction_hash,
    s.is_buy,
    s.token_amount,
    s.native_amount,
    s.before_token_amount,
    s.before_native_amount,
    s.created_at,
    t.name,
    t.symbol,
    t.image_uri,
    ax.x_handle,
    ax.x_image_uri,
    ax.is_blue_label
FROM swap s
JOIN token t ON s.token_id = t.token_id
LEFT JOIN account_x ax ON s.account_id = ax.account_id
WHERE s.account_id = $1
ORDER BY s.created_at DESC
LIMIT $2
OFFSET $3
```

### 3. Get Token Swaps (Dynamic Query)
```sql
SELECT 
    s.swap_id,
    s.token_id,
    s.transaction_hash,
    s.is_buy,
    s.token_amount,
    s.native_amount,
    s.before_token_amount,
    s.before_native_amount,
    s.created_at,
    a.account_id,
    a.nickname,
    a.image_uri,
    a.follower_count,
    a.following_count,
    ax.x_handle,
    ax.x_image_uri,
    ax.is_blue_label
FROM swap s
JOIN account a ON s.account_id = a.account_id
LEFT JOIN account_x ax ON s.account_id = ax.account_id
WHERE s.token_id = $1
[AND s.native_amount >= $X] -- Optional
[AND s.account_id = $X] -- Optional
[AND s.is_buy = $X] -- Optional
ORDER BY s.created_at [DESC/ASC]
LIMIT $X
OFFSET $X
```

### 4. Get Token Swap Count (Dynamic Query)
```sql
SELECT COALESCE(COUNT(*)::bigint, 0) as count
FROM swap s
JOIN account a ON s.account_id = a.account_id
WHERE s.token_id = $1
[AND s.native_amount >= $X] -- Optional
[AND s.account_id = $X] -- Optional
[AND s.is_buy = $X] -- Optional
```

### 5. Get Cached Swap Count
```sql
SELECT [column_name] as count FROM swap_count WHERE token_id = $1
```

---

## Trading Position Controller
**File**: `/src/types/trading/position.rs`

### 1. Get Token Holders Count
```sql
SELECT COALESCE(COUNT(*)::bigint, 0) as count
FROM balance b
WHERE b.token_id = $1 AND b.balance > 0
```

### 2. Get Token Holder Details
```sql
SELECT 
    b.balance as current_token_amount,
    a.account_id,
    a.nickname,
    a.image_uri,
    a.follower_count,
    a.following_count,
    ax.x_handle,
    ax.x_image_uri,
    ax.is_blue_label
FROM balance b
JOIN account a ON b.account_id = a.account_id
LEFT JOIN account_x ax ON b.account_id = ax.account_id
WHERE b.token_id = $1 AND b.balance > 0
ORDER BY b.balance DESC
LIMIT $2 OFFSET $3
```

### 3. Get Token Creator
```sql
SELECT 
    t.creator as "creator!"
FROM token t
WHERE t.token_id = $1
```

### 4. Get Account Positions Count
```sql
SELECT COALESCE(COUNT(*)::bigint, 0) as count
FROM balance b
WHERE b.account_id = $1 AND b.balance > 0
```

### 5. Get Account Positions
```sql
SELECT 
    t.token_id,
    t.name,
    t.symbol,
    t.image_uri,
    b.balance as current_amount,
    m.price as price,
    COALESCE(m.price * b.balance, 0) as value
FROM balance b
JOIN token t ON b.token_id = t.token_id
JOIN market m ON b.token_id = m.token_id
WHERE b.account_id = $1 AND b.balance > 0
ORDER BY value DESC
LIMIT $2 OFFSET $3
```

---

## Trading Price Controller
**File**: `/src/types/trading/price.rs`

### 1. Get Token Price
```sql
SELECT 
    COALESCE(m.price, 0)::numeric as "price!"
FROM market m
WHERE m.token_id = $1
```

---

## Trading Market Controller
**File**: `/src/types/trading/market.rs`

### 1. Get Market Data
```sql
SELECT 
    market_type,
    token_id,
    reserve_native,
    reserve_token,
    total_supply,
    price,
    market_cap,
    latest_trade_at
FROM market
WHERE token_id = $1
```

---

## Trading Chart Controller
**File**: `/src/types/trading/chart.rs`

### 1. Get Chart Data
```sql
SELECT 
    interval_type,
    token_id,                           
    time_stamp,
    open_price,
    high_price,
    low_price,
    close_price,
    volume,
    created_at
FROM chart
WHERE token_id = $1 AND interval_type = $2
ORDER BY time_stamp DESC
LIMIT $3
```

---

## New Content Controller
**File**: `/src/types/new_content/mod.rs`

### 1. Get Latest New Buy
```sql
SELECT
    s.is_buy,
    s.native_amount,
    a.nickname,
    a.image_uri,
    a.follower_count,
    a.following_count,
    t.name as token_name,
    t.symbol as token_symbol,
    t.image_uri as token_image_uri,
    t.token_id as token_id,
    a.account_id as account_id,
    ax.x_handle,
    ax.x_image_uri
FROM swap s
JOIN account a ON s.account_id = a.account_id
JOIN token t ON s.token_id = t.token_id
LEFT JOIN account_x ax ON s.account_id = ax.account_id
WHERE s.is_buy = true
ORDER BY s.created_at DESC
LIMIT 1
```

### 2. Get Latest New Sell
```sql
SELECT
    s.is_buy,
    s.native_amount,
    a.nickname,
    a.image_uri,
    a.follower_count,
    a.following_count,
    t.name as token_name,
    t.symbol as token_symbol,
    t.image_uri as token_image_uri,
    t.token_id as token_id,
    a.account_id as account_id,
    ax.x_handle,
    ax.x_image_uri
FROM swap s
JOIN account a ON s.account_id = a.account_id
JOIN token t ON s.token_id = t.token_id
LEFT JOIN account_x ax ON s.account_id = ax.account_id
WHERE s.is_buy = false
ORDER BY s.created_at DESC
LIMIT 1
```

### 3. Get Latest New Token
```sql
SELECT 
    t.name as token_name,
    t.symbol,
    t.image_uri as token_image_uri,
    t.created_at,
    t.token_id as token_id,
    a.nickname,
    a.image_uri,
    a.account_id,
    a.follower_count,
    a.following_count,
    ax.x_handle,
    ax.x_image_uri
FROM token t
JOIN account a ON t.creator = a.account_id
LEFT JOIN account_x ax ON t.creator = ax.account_id
ORDER BY t.created_at DESC
LIMIT 1
```

---

## Token Management Controller
**File**: `/src/types/management/mod.rs`

### 1. Get Dev Positions
```sql
SELECT 
    t.token_id,
    t.name,
    t.symbol,
    t.image_uri,
    t.total_supply,
    m.price,
    COALESCE(m.price * t.total_supply, 0) as market_cap,
    COALESCE(l.treasury_amount, 0) as treasury_amount,
    COALESCE(b.balance, 0) as balance,
    COALESCE(f.fee_amount, 0) as fee_amount
FROM token t
LEFT JOIN market m ON t.token_id = m.token_id
LEFT JOIN liquidity l ON t.token_id = l.token_id
LEFT JOIN balance b ON t.token_id = b.token_id AND b.account_id = $1
LEFT JOIN token_fee f ON t.token_id = f.token_id
WHERE t.creator = $1
ORDER BY m.price * t.total_supply [DESC/ASC]
LIMIT $X OFFSET $X
```

### 2. Get Dev Positions Count
```sql
SELECT 
    COUNT(*) as count
FROM 
    token t
WHERE t.creator = $1
```

### 3. Get Holding Token Management
```sql
SELECT 
    t.token_id,
    t.name,
    t.symbol,
    t.image_uri,
    t.total_supply,
    m.price,
    m.price * t.total_supply as market_cap,
    COALESCE(l.treasury_amount, 0) as treasury_amount,
    COALESCE(b.balance, 0) as balance
FROM token t
LEFT JOIN market m ON t.token_id = m.token_id
LEFT JOIN liquidity l ON t.token_id = l.token_id
JOIN balance b ON t.token_id = b.token_id
WHERE b.account_id = $1 AND b.balance > 0
ORDER BY b.balance * m.price [DESC/ASC]
LIMIT $X OFFSET $X
```

### 4. Get Holding Token Management Count
```sql
SELECT 
    COUNT(*)
FROM 
    balance b
WHERE 
    b.account_id = $1 
    AND b.balance > 0
```

### 5. Get Token Locks
```sql
SELECT 
    t.token_id,
    t.name,
    t.symbol,
    t.image_uri,
    t.total_supply,
    m.price,
    m.price * t.total_supply as market_cap,
    COALESCE(tl.locked_amount, 0) as locked_amount,
    tl.lock_id,
    tl.lock_at,
    tl.unlock_at
FROM token_lock tl
JOIN token t ON tl.token_id = t.token_id
LEFT JOIN market m ON t.token_id = m.token_id
WHERE tl.account_id = $1 AND tl.is_withdrawed = false
ORDER BY tl.unlock_at ASC, locked_amount * m.price DESC
LIMIT $X OFFSET $X
```

### 6. Get Token Locks Count
```sql
SELECT 
    COUNT(*) as count
FROM 
    token_lock tl
WHERE 
    tl.account_id = $1 
    AND tl.is_withdrawed = false
```

### 7. Get Withdrawable Locks
```sql
SELECT 
    t.token_id,
    t.name,
    t.symbol,
    t.image_uri,
    t.total_supply,
    m.price,
    m.price * t.total_supply as market_cap,
    tl.lock_id,
    tl.locked_amount
FROM token_lock tl
JOIN token t ON tl.token_id = t.token_id
LEFT JOIN market m ON t.token_id = m.token_id
WHERE tl.account_id = $1 
AND tl.is_withdrawed = false
AND tl.unlock_at <= $2
ORDER BY tl.locked_amount * m.price DESC
LIMIT $X OFFSET $X
```

### 8. Get Withdrawable Locks Count
```sql
SELECT 
    COUNT(DISTINCT token_id) as count
FROM 
    token_lock tl
WHERE 
    tl.account_id = $1 
    AND tl.is_withdrawed = false
    AND tl.unlock_at <= $2
```

### 9. Get Token Management History (Dynamic Query)
```sql
SELECT 
    th.token_amount,
    th.history_type AS activity,
    th.before_amount,
    th.created_at,
    t.token_id,
    t.name,
    t.symbol,
    t.image_uri,
    a.account_id,
    a.nickname,
    a.image_uri AS account_image_uri,
    a.follower_count,
    a.following_count,
    ax.x_handle,
    ax.x_image_uri,
    ax.is_blue_label
FROM token_management_history th
JOIN token t ON th.token_id = t.token_id
JOIN account a ON th.account_id = a.account_id
LEFT JOIN account_x ax ON a.account_id = ax.account_id
WHERE th.token_id = $1
[AND th.token_amount >= $X] -- Optional
[AND th.account_id = $X] -- Optional
[AND th.history_type = $X] -- Optional
ORDER BY th.created_at [DESC/ASC]
LIMIT $X
OFFSET $X
```

### 10. Get Token Management History Count (Dynamic Query)
```sql
SELECT COALESCE(COUNT(*)::bigint, 0) as count
FROM token_management_history th
JOIN account a ON th.account_id = a.account_id
WHERE th.token_id = $1
[AND th.token_amount >= $X] -- Optional
[AND th.account_id = $X] -- Optional
[AND th.history_type = $X] -- Optional
```

### 11. Get Cached Management History Count
```sql
SELECT [column_name] as count FROM token_management_history_count WHERE token_id = $1
```

### 12. Get Cached Total Management History Count
```sql
SELECT 
    total_count as count 
FROM token_management_history_count
WHERE token_id = $1
```

---

## Summary

This document contains all database queries found in the types directory controllers. The queries are organized by controller for easy reference and testing.

### Key Patterns Observed:
1. Most queries use prepared statements with parameter binding for security
2. Timeouts are consistently set to 500ms
3. LEFT JOINs are used extensively for optional data (e.g., account_x table)
4. COALESCE is used frequently to handle NULL values
5. CTEs (Common Table Expressions) are used for complex queries
6. Dynamic query building is used for flexible filtering
7. Pagination is implemented consistently across list endpoints
8. Caching tables are used for performance optimization (swap_count, token_management_history_count)