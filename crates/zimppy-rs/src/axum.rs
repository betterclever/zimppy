//! Axum integration — `ChargeChallenger` implementation for Zcash.
//!
//! Enables the `MppCharge<C>` extractor pattern from mpp-rs with Zcash
//! shielded payments.
//!
//! # Example
//!
//! ```ignore
//! use mpp::server::axum::{ChargeConfig, MppCharge, ChargeChallenger, WithReceipt};
//! use zimppy_rs::axum::ZcashChallenger;
//! use zimppy_rs::ZcashChargeMethod;
//!
//! struct FortunePrice;
//! impl ChargeConfig for FortunePrice {
//!     fn amount() -> &'static str { "42000" }
//! }
//!
//! let challenger = ZcashChallenger::new(
//!     ZcashChargeMethod::new(&rpc, &address, &ivk),
//!     "zimppy", &secret, &address, "testnet",
//! );
//! let state: Arc<dyn ChargeChallenger> = Arc::new(challenger);
//!
//! let app = Router::new()
//!     .route("/api/fortune", get(fortune))
//!     .with_state(state);
//!
//! async fn fortune(
//!     charge: MppCharge<FortunePrice>,
//! ) -> WithReceipt<Json<Value>> {
//!     WithReceipt {
//!         receipt: charge.receipt,
//!         body: Json(json!({ "fortune": "..." })),
//!     }
//! }
//! ```

use std::pin::Pin;

use mpp::compute_challenge_id;
use mpp::parse_authorization;
use mpp::protocol::core::{Base64UrlJson, PaymentChallenge, Receipt};
use mpp::protocol::intents::ChargeRequest;
use mpp::server::axum::{ChallengeOptions, ChargeChallenger};
use mpp::server::Mpp;

use crate::ZcashChargeMethod;

/// Zcash implementation of `ChargeChallenger` for the mpp-rs axum extractor.
pub struct ZcashChallenger<S = ()> {
    mpp: Mpp<ZcashChargeMethod, S>,
    secret_key: String,
    realm: String,
    address: String,
    network: String,
}

impl ZcashChallenger<()> {
    /// Create a new challenger without session support.
    pub fn new(
        charge: ZcashChargeMethod,
        realm: &str,
        secret: &str,
        address: &str,
        network: &str,
    ) -> Self {
        Self {
            mpp: Mpp::new(charge, realm, secret),
            secret_key: secret.to_string(),
            realm: realm.to_string(),
            address: address.to_string(),
            network: network.to_string(),
        }
    }
}

impl<S> ZcashChallenger<S> {
    /// Create from an existing Mpp instance.
    pub fn from_mpp(
        mpp: Mpp<ZcashChargeMethod, S>,
        secret: &str,
        realm: &str,
        address: &str,
        network: &str,
    ) -> Self {
        Self {
            mpp,
            secret_key: secret.to_string(),
            realm: realm.to_string(),
            address: address.to_string(),
            network: network.to_string(),
        }
    }

    /// Access the inner Mpp (for session methods, etc.)
    pub fn mpp(&self) -> &Mpp<ZcashChargeMethod, S> {
        &self.mpp
    }
}

impl<S: Clone + Send + Sync + 'static> ChargeChallenger for ZcashChallenger<S> {
    fn challenge(
        &self,
        amount: &str,
        options: ChallengeOptions,
    ) -> Result<PaymentChallenge, String> {
        let request = ChargeRequest {
            amount: amount.to_string(),
            currency: "zec".to_string(),
            recipient: Some(self.address.clone()),
            description: options.description.map(|s| s.to_string()),
            method_details: Some(serde_json::json!({
                "memo": "zimppy:{id}",
                "network": self.network,
            })),
            ..Default::default()
        };

        let encoded =
            Base64UrlJson::from_typed(&request).map_err(|e| format!("encode request: {e}"))?;

        let expires = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::seconds(600))
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339();

        let id = compute_challenge_id(
            &self.secret_key,
            &self.realm,
            "zcash",
            "charge",
            encoded.raw(),
            Some(&expires),
            None,
            None,
        );

        Ok(PaymentChallenge {
            id,
            realm: self.realm.clone(),
            method: "zcash".into(),
            intent: "charge".into(),
            request: encoded,
            expires: Some(expires),
            description: options.description.map(|s| s.to_string()),
            digest: None,
            opaque: None,
        })
    }

    fn verify_payment(
        &self,
        credential_str: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Receipt, String>> + Send>> {
        let credential = match parse_authorization(credential_str) {
            Ok(c) => c,
            Err(e) => {
                return Box::pin(std::future::ready(Err(format!(
                    "Invalid credential: {e}"
                ))))
            }
        };
        let mpp = self.mpp.clone();
        Box::pin(async move {
            mpp.verify_credential(&credential)
                .await
                .map_err(|e| e.to_string())
        })
    }
}
