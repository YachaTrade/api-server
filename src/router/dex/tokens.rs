use axum::{
    Json,
    extract::{Query, State},
};
use tracing::instrument;

use super::path::DexPath;
use crate::{
    result::{AppError, AppJsonResult},
    services::dex::DexService,
    state::AppState,
    types::dex::tokens::{DexTokenListQuery, DexTokenListResponse},
    utils::{normalize_ca_query, valid_account_id},
};

/// V2 DEX-tradeable token list for the Select Token modal. Without `q`: a
/// 4-tier list (held whitelist → held nadfun V2 → whitelist → nadfun V2;
/// held tiers require `?account=`). With `q`: case-insensitive prefix search
/// on symbol/name/CA; external tokens surface only on an exact full contract
/// address. Optionally attaches per-account balance/USD when `?account=` is
/// provided.
///
/// Each entry's `token_type` ∈ `whitelist` | `nadfun_v2` | `external`
/// (테이블 멤버십 분류: whitelist_token / token(nadfun_v2) / 그 외 external).
/// 기본 리스트는 `whitelist`+`nadfun_v2`만, `external`은 검색에서만 노출.
/// 값별 의미는 `DexTokenType` 스키마 참고.
#[utoipa::path(
    get,
    path = DexPath::GetTokens.docs_str(),
    params(DexTokenListQuery),
    responses(
        (status = 200, description = "Paginated token list with total_count", body = DexTokenListResponse),
        (status = 400, description = "Invalid query (bad account address, page < 1, or limit out of 1..=100)"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Dex"
)]
#[instrument(skip(state))]
pub async fn get_tokens(
    State(state): State<AppState>,
    Query(mut params): Query<DexTokenListQuery>,
) -> AppJsonResult<DexTokenListResponse> {
    // Normalize account to EIP-55 checksum if provided.
    if let Some(raw) = params.account.as_deref() {
        params.account = Some(
            valid_account_id(raw)
                .ok_or_else(|| AppError::BadRequest("Invalid account id".to_string()))?,
        );
    }

    // Normalize a full contract-address search term to EIP-55 checksum so exact
    // token_id matching works (handles 0x/0X and any input casing). Non-CA terms
    // (symbol/name, partial CA) are passed through untouched.
    params.q = params.q.as_deref().map(normalize_ca_query);

    let service = DexService::new(state.postgres.clone());
    let response = service.get_tokens(&params).await?;

    Ok(Json(response))
}
