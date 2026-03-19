use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub type DynError = Box<dyn Error + Send + Sync>;

const MEMO_PREFIX: &str = "zimppy:";
const ZCASH_MEMO_BYTES: usize = 512;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalAppConfig {
    pub project_name: String,
    pub ports: LocalPorts,
    pub storage: StorageConfig,
    pub services: LocalServices,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalPorts {
    pub api: u16,
    pub backend: u16,
    pub test_helper: u16,
    pub integration_harness: u16,
    pub lightwalletd_tunnel: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StorageConfig {
    pub state_directory: String,
    pub sqlite_file: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalServices {
    pub api_base_url: String,
    pub backend_base_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RemoteChainServiceConfig {
    pub network: ZcashNetwork,
    pub lightwalletd: LightwalletdConfig,
    pub upstream: UpstreamConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LightwalletdConfig {
    pub access: String,
    pub host: String,
    pub port: u16,
    pub endpoint: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamConfig {
    pub host_alias: String,
    pub remote_port: u16,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub local_app: LocalAppConfig,
    pub remote_chain_service: RemoteChainServiceConfig,
    pub backend_port: u16,
    pub state_directory: PathBuf,
}

impl RuntimeConfig {
    pub fn load() -> Result<Self, DynError> {
        let repo_root = repo_root();
        let local_app: LocalAppConfig = read_json(repo_root.join("config/local-app.json"))?;
        let remote_chain_service: RemoteChainServiceConfig =
            read_json(repo_root.join("config/remote-chain-service.json"))?;
        let backend_port = std::env::var("PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(local_app.ports.backend);
        let state_directory = repo_root.join(&local_app.storage.state_directory);

        fs::create_dir_all(&state_directory)?;

        Ok(Self {
            local_app,
            remote_chain_service,
            backend_port,
            state_directory,
        })
    }

    #[must_use]
    pub fn repo_root(&self) -> PathBuf {
        repo_root()
    }

    pub fn health_body(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&serde_json::json!({
            "service": "zimppy-backend",
            "status": "ok",
            "project": self.local_app.project_name,
            "ports": {
                "api": self.local_app.ports.api,
                "backend": self.backend_port,
                "testHelper": self.local_app.ports.test_helper,
                "integrationHarness": self.local_app.ports.integration_harness,
                "lightwalletdTunnel": self.local_app.ports.lightwalletd_tunnel,
            },
            "remoteChainService": {
                "network": self.remote_chain_service.network,
                "endpoint": self.remote_chain_service.lightwalletd.endpoint,
            },
            "storage": {
                "stateDirectory": self.state_directory.display().to_string(),
                "sqliteFile": self.local_app.storage.sqlite_file,
            }
        }))
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn read_json<T>(path: impl AsRef<Path>) -> Result<T, DynError>
where
    T: for<'de> Deserialize<'de>,
{
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ZcashNetwork {
    Testnet,
}

impl fmt::Display for ZcashNetwork {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Testnet => f.write_str("testnet"),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReceiverKind {
    TransparentP2pkh,
    Sapling,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Address {
    pub network: ZcashNetwork,
    pub kind: ReceiverKind,
    pub value: String,
}

impl Address {
    pub fn new(
        network: ZcashNetwork,
        kind: ReceiverKind,
        value: impl Into<String>,
    ) -> Result<Self, AddressError> {
        let value = value.into();
        let expected_prefix = match (network, kind) {
            (ZcashNetwork::Testnet, ReceiverKind::TransparentP2pkh) => "tm",
            (ZcashNetwork::Testnet, ReceiverKind::Sapling) => "ztestsapling",
        };

        if !value.starts_with(expected_prefix) {
            return Err(AddressError::UnexpectedPrefix {
                expected: expected_prefix,
                actual: value,
            });
        }

        Ok(Self {
            network,
            kind,
            value,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressError {
    UnexpectedPrefix {
        expected: &'static str,
        actual: String,
    },
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedPrefix { expected, actual } => {
                write!(f, "expected address with prefix {expected}, got {actual}")
            }
        }
    }
}

impl Error for AddressError {}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ChallengeBinding {
    pub challenge_id: String,
    pub request_binding_hash: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct MemoPayload {
    pub challenge_id: String,
    pub request_binding_hash: String,
    pub recipient_alias: String,
}

impl MemoPayload {
    pub fn from_binding(binding: &ChallengeBinding, recipient_alias: impl Into<String>) -> Self {
        Self {
            challenge_id: binding.challenge_id.clone(),
            request_binding_hash: binding.request_binding_hash.clone(),
            recipient_alias: recipient_alias.into(),
        }
    }

    pub fn encode(&self) -> Result<String, MemoError> {
        let json = serde_json::to_vec(self).map_err(MemoError::Serialize)?;
        let encoded = format!("{MEMO_PREFIX}{}", hex_encode(&json));

        if encoded.len() > ZCASH_MEMO_BYTES {
            return Err(MemoError::TooLong {
                bytes: encoded.len(),
                limit: ZCASH_MEMO_BYTES,
            });
        }

        Ok(encoded)
    }

    pub fn decode(encoded: &str) -> Result<Self, MemoError> {
        let hex = encoded
            .strip_prefix(MEMO_PREFIX)
            .ok_or(MemoError::MissingPrefix)?;
        let bytes = hex_decode(hex)?;
        serde_json::from_slice(&bytes).map_err(MemoError::Deserialize)
    }
}

#[derive(Debug)]
pub enum MemoError {
    MissingPrefix,
    InvalidHex(String),
    Serialize(serde_json::Error),
    Deserialize(serde_json::Error),
    TooLong { bytes: usize, limit: usize },
}

impl fmt::Display for MemoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPrefix => write!(f, "memo payload missing {MEMO_PREFIX} prefix"),
            Self::InvalidHex(value) => write!(f, "memo payload contains invalid hex: {value}"),
            Self::Serialize(error) => write!(f, "failed to serialize memo payload: {error}"),
            Self::Deserialize(error) => write!(f, "failed to deserialize memo payload: {error}"),
            Self::TooLong { bytes, limit } => {
                write!(f, "memo payload exceeds {limit} byte limit: {bytes}")
            }
        }
    }
}

impl Error for MemoError {}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TransparentReceivingTerms {
    pub network: ZcashNetwork,
    pub recipient: Address,
    pub amount_zat: u64,
    pub challenge_id: String,
    pub verifier: ChainVerifierLocator,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ShieldedReceivingTerms {
    pub network: ZcashNetwork,
    pub recipient: Address,
    pub amount_zat: u64,
    pub challenge_id: String,
    pub memo: String,
    pub verifier: ChainVerifierLocator,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ChainVerifierLocator {
    pub service: ChainService,
    pub endpoint: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChainService {
    RemoteLightwalletd,
}

impl ChainVerifierLocator {
    #[must_use]
    pub fn from_config(config: &RemoteChainServiceConfig) -> Self {
        Self {
            service: ChainService::RemoteLightwalletd,
            endpoint: config.lightwalletd.endpoint.clone(),
        }
    }
}

pub fn build_transparent_terms(
    config: &RemoteChainServiceConfig,
    recipient: Address,
    amount_zat: u64,
    challenge_id: impl Into<String>,
) -> TransparentReceivingTerms {
    TransparentReceivingTerms {
        network: config.network,
        recipient,
        amount_zat,
        challenge_id: challenge_id.into(),
        verifier: ChainVerifierLocator::from_config(config),
    }
}

pub fn build_shielded_terms(
    config: &RemoteChainServiceConfig,
    recipient: Address,
    amount_zat: u64,
    binding: &ChallengeBinding,
    recipient_alias: impl Into<String>,
) -> Result<ShieldedReceivingTerms, MemoError> {
    let memo = MemoPayload::from_binding(binding, recipient_alias).encode()?;

    Ok(ShieldedReceivingTerms {
        network: config.network,
        recipient,
        amount_zat,
        challenge_id: binding.challenge_id.clone(),
        memo,
        verifier: ChainVerifierLocator::from_config(config),
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TransparentPaymentProof {
    pub txid: String,
    pub output_index: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ShieldedPaymentProof {
    pub txid: String,
    pub account_index: u32,
    pub expected_memo: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct VerificationOutcome {
    pub txid: String,
    pub observed_recipient: String,
    pub observed_amount_zat: u64,
    pub binding: Option<ChallengeBinding>,
}

pub trait LightwalletdVerificationClient {
    fn endpoint(&self) -> &str;

    fn verify_transparent_payment(
        &self,
        proof: &TransparentPaymentProof,
        terms: &TransparentReceivingTerms,
    ) -> Result<VerificationOutcome, VerificationError>;

    fn verify_shielded_payment(
        &self,
        proof: &ShieldedPaymentProof,
        terms: &ShieldedReceivingTerms,
    ) -> Result<VerificationOutcome, VerificationError>;
}

#[derive(Debug, Clone)]
pub struct RemoteLightwalletdVerifier {
    endpoint: String,
}

impl RemoteLightwalletdVerifier {
    #[must_use]
    pub fn new(config: &RemoteChainServiceConfig) -> Self {
        Self {
            endpoint: config.lightwalletd.endpoint.clone(),
        }
    }
}

impl LightwalletdVerificationClient for RemoteLightwalletdVerifier {
    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn verify_transparent_payment(
        &self,
        _proof: &TransparentPaymentProof,
        _terms: &TransparentReceivingTerms,
    ) -> Result<VerificationOutcome, VerificationError> {
        Err(VerificationError::UnimplementedRemoteLookup {
            endpoint: self.endpoint.clone(),
            flow: "transparent",
        })
    }

    fn verify_shielded_payment(
        &self,
        _proof: &ShieldedPaymentProof,
        _terms: &ShieldedReceivingTerms,
    ) -> Result<VerificationOutcome, VerificationError> {
        Err(VerificationError::UnimplementedRemoteLookup {
            endpoint: self.endpoint.clone(),
            flow: "shielded",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationError {
    UnimplementedRemoteLookup {
        endpoint: String,
        flow: &'static str,
    },
}

impl fmt::Display for VerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnimplementedRemoteLookup { endpoint, flow } => {
                write!(
                    f,
                    "{flow} verification requires remote lightwalletd lookup via {endpoint}"
                )
            }
        }
    }
}

impl Error for VerificationError {}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn hex_decode(value: &str) -> Result<Vec<u8>, MemoError> {
    if !value.len().is_multiple_of(2) {
        return Err(MemoError::InvalidHex(value.to_string()));
    }

    let mut bytes = Vec::with_capacity(value.len() / 2);
    let chars = value.as_bytes().chunks_exact(2);

    for pair in chars {
        let pair =
            std::str::from_utf8(pair).map_err(|_| MemoError::InvalidHex(value.to_string()))?;
        let byte =
            u8::from_str_radix(pair, 16).map_err(|_| MemoError::InvalidHex(value.to_string()))?;
        bytes.push(byte);
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{
        build_shielded_terms, build_transparent_terms, Address, ChainService, ChallengeBinding,
        LightwalletdVerificationClient, MemoError, MemoPayload, ReceiverKind,
        RemoteLightwalletdVerifier, RuntimeConfig, VerificationError, ZcashNetwork,
    };

    #[test]
    fn loads_backend_runtime_config_with_reserved_ports() {
        let config = RuntimeConfig::load().unwrap_or_else(|error| {
            panic!("runtime config should load: {error}");
        });

        assert_eq!(config.local_app.project_name, "zimppy");
        assert_eq!(config.local_app.ports.backend, 3181);
        assert_eq!(config.local_app.ports.lightwalletd_tunnel, 3184);
        assert_eq!(config.remote_chain_service.network, ZcashNetwork::Testnet);
        assert!(config.state_directory.ends_with(".local/state/zimppy"));
    }

    #[test]
    fn transparent_terms_carry_testnet_recipient_amount_and_remote_verifier() {
        let config = RuntimeConfig::load().unwrap_or_else(|error| {
            panic!("runtime config should load: {error}");
        });
        let recipient = Address::new(
            ZcashNetwork::Testnet,
            ReceiverKind::TransparentP2pkh,
            "tmYd5nFLM8ptuA6A9LTqCVhGfX3Wb5f4K8p",
        )
        .unwrap_or_else(|error| panic!("transparent address should be accepted: {error}"));

        let terms = build_transparent_terms(
            &config.remote_chain_service,
            recipient.clone(),
            42_000,
            "challenge-123",
        );

        assert_eq!(terms.network, ZcashNetwork::Testnet);
        assert_eq!(terms.recipient, recipient);
        assert_eq!(terms.amount_zat, 42_000);
        assert_eq!(terms.challenge_id, "challenge-123");
        assert_eq!(terms.verifier.service, ChainService::RemoteLightwalletd);
        assert_eq!(terms.verifier.endpoint, "http://127.0.0.1:3184");
    }

    #[test]
    fn shielded_terms_embed_decodable_memo_binding() {
        let config = RuntimeConfig::load().unwrap_or_else(|error| {
            panic!("runtime config should load: {error}");
        });
        let recipient = Address::new(
            ZcashNetwork::Testnet,
            ReceiverKind::Sapling,
            "ztestsapling1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
        )
        .unwrap_or_else(|error| panic!("shielded address should be accepted: {error}"));
        let binding = ChallengeBinding {
            challenge_id: "challenge-123".to_string(),
            request_binding_hash: "2f4d9d9e".to_string(),
        };

        let terms = build_shielded_terms(
            &config.remote_chain_service,
            recipient.clone(),
            99_000,
            &binding,
            "merchant-sapling",
        )
        .unwrap_or_else(|error| panic!("shielded terms should be built: {error}"));
        let decoded = MemoPayload::decode(&terms.memo)
            .unwrap_or_else(|error| panic!("memo should decode: {error}"));

        assert_eq!(terms.network, ZcashNetwork::Testnet);
        assert_eq!(terms.recipient, recipient);
        assert_eq!(terms.amount_zat, 99_000);
        assert_eq!(terms.challenge_id, binding.challenge_id);
        assert_eq!(decoded.challenge_id, "challenge-123");
        assert_eq!(decoded.request_binding_hash, "2f4d9d9e");
        assert_eq!(decoded.recipient_alias, "merchant-sapling");
        assert_eq!(terms.verifier.endpoint, "http://127.0.0.1:3184");
    }

    #[test]
    fn memo_payload_rejects_oversized_binding() {
        let long_alias = "a".repeat(300);
        let payload = MemoPayload {
            challenge_id: "challenge-123".to_string(),
            request_binding_hash: "2f4d9d9e".to_string(),
            recipient_alias: long_alias,
        };

        let error = payload.encode().expect_err("oversized memo should fail");

        assert!(matches!(error, MemoError::TooLong { .. }));
    }

    #[test]
    fn memo_payload_rejects_missing_prefix() {
        let error = MemoPayload::decode("bad-prefix").expect_err("invalid memo should fail");

        assert!(matches!(error, MemoError::MissingPrefix));
    }

    #[test]
    fn remote_lightwalletd_verifier_exposes_remote_boundary_without_local_node_assumptions() {
        let config = RuntimeConfig::load().unwrap_or_else(|error| {
            panic!("runtime config should load: {error}");
        });
        let verifier = RemoteLightwalletdVerifier::new(&config.remote_chain_service);
        let recipient = Address::new(
            ZcashNetwork::Testnet,
            ReceiverKind::TransparentP2pkh,
            "tmYd5nFLM8ptuA6A9LTqCVhGfX3Wb5f4K8p",
        )
        .unwrap_or_else(|error| panic!("transparent address should be accepted: {error}"));
        let terms = build_transparent_terms(
            &config.remote_chain_service,
            recipient,
            42_000,
            "challenge-123",
        );
        let error = verifier
            .verify_transparent_payment(
                &super::TransparentPaymentProof {
                    txid: "00".repeat(32),
                    output_index: 0,
                },
                &terms,
            )
            .expect_err("stub verifier should signal remote lookup boundary");

        assert_eq!(verifier.endpoint(), "http://127.0.0.1:3184");
        assert_eq!(
            error,
            VerificationError::UnimplementedRemoteLookup {
                endpoint: "http://127.0.0.1:3184".to_string(),
                flow: "transparent",
            }
        );
    }
}
