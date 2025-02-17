pub mod handler;
pub mod path;
use crate::state::AppState;

use axum::{
    routing::{get, post},
    Router,
};

use path::ReferralPath;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            ReferralPath::CheckReigsterReferral.as_str(),
            get(handler::check_register_referral_code),
        )
        .route(
            ReferralPath::RegisterReferral.as_str(),
            post(handler::register_referral_code),
        )
        .route(
            ReferralPath::MakeReferralCode.as_str(),
            post(handler::make_referral_code),
        )
        .route(
            ReferralPath::ExistsReferralCode.as_str(),
            get(handler::exists_referral_code),
        )
}
