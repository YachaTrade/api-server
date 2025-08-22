-- Verified Token Order Query Performance Test
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
SELECT 
    t.token_id, a.account_id, a.nickname, a.image_uri as account_image_uri,
    a.follower_count, a.following_count, t.name, t.symbol,
    t.image_uri as token_image_uri, t.description,
    t.total_supply as total_supply,
    m.price,
    m.reserve_token,
    CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
    ax.x_image_uri,
    ax.is_blue_label,
    m.market_type, t.created_at, m.price::FLOAT8 as score
FROM token t
JOIN account a ON t.creator = a.account_id
LEFT JOIN account_x ax ON t.creator = ax.account_id
LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
JOIN market m ON t.token_id = m.token_id
WHERE av.account_id IS NOT NULL
ORDER BY m.price DESC
LIMIT 20 OFFSET 0;