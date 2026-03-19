use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Zebrad JSON-RPC 2.0 client.
#[derive(Clone)]
pub struct ZebradRpc {
    endpoint: String,
    client: reqwest::Client,
}

impl ZebradRpc {
    pub fn new(endpoint: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();
        Self {
            endpoint: endpoint.to_string(),
            client,
        }
    }

    /// Verbose mode — returns structured JSON with vout[], confirmations.
    /// Used for transparent verification.
    pub async fn get_transaction_verbose(
        &self,
        txid: &str,
    ) -> Result<VerboseTransaction, RpcError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "getrawtransaction",
            "params": [txid, 1],
            "id": 1,
        });
        let resp: JsonRpcResponse<VerboseTransaction> = self.call(&body).await?;
        resp.into_result()
    }

    /// Raw hex mode — returns raw tx bytes as hex string.
    /// Used for shielded verification where we need to decrypt outputs.
    pub async fn get_raw_transaction_hex(&self, txid: &str) -> Result<String, RpcError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "getrawtransaction",
            "params": [txid, 0],
            "id": 1,
        });
        let resp: JsonRpcResponse<String> = self.call(&body).await?;
        resp.into_result()
    }

    /// Returns txids in mempool.
    pub async fn get_raw_mempool(&self) -> Result<Vec<String>, RpcError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "getrawmempool",
            "params": [],
            "id": 1,
        });
        let resp: JsonRpcResponse<Vec<String>> = self.call(&body).await?;
        resp.into_result()
    }

    /// Submit signed transaction.
    pub async fn send_raw_transaction(&self, hex: &str) -> Result<String, RpcError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "sendrawtransaction",
            "params": [hex],
            "id": 1,
        });
        let resp: JsonRpcResponse<String> = self.call(&body).await?;
        resp.into_result()
    }

    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        body: &serde_json::Value,
    ) -> Result<JsonRpcResponse<T>, RpcError> {
        let resp = self
            .client
            .post(&self.endpoint)
            .json(body)
            .send()
            .await
            .map_err(|e| RpcError::Network(e.to_string()))?;

        let status = resp.status();
        if status.as_u16() == 429 {
            return Err(RpcError::RateLimited);
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(RpcError::HttpError {
                status: status.as_u16(),
                body: text,
            });
        }

        resp.json::<JsonRpcResponse<T>>()
            .await
            .map_err(|e| RpcError::Parse(e.to_string()))
    }
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<JsonRpcError>,
    #[allow(dead_code)]
    id: Option<serde_json::Value>,
}

impl<T> JsonRpcResponse<T> {
    fn into_result(self) -> Result<T, RpcError> {
        if let Some(error) = self.error {
            return Err(RpcError::Rpc {
                code: error.code,
                message: error.message,
            });
        }
        self.result.ok_or(RpcError::EmptyResponse)
    }
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

/// Verbose transaction data from getrawtransaction(txid, 1).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VerboseTransaction {
    pub txid: Option<String>,
    pub confirmations: Option<u32>,
    pub vout: Option<Vec<TransparentOutput>>,
}

/// A transparent output in a Zcash transaction.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransparentOutput {
    /// Amount in ZEC.
    pub value: Option<f64>,
    /// Amount in zatoshis.
    pub value_zat: Option<u64>,
    /// Output index.
    pub n: Option<u32>,
    /// Script public key details.
    #[serde(rename = "scriptPubKey")]
    pub script_pub_key: Option<ScriptPubKey>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScriptPubKey {
    #[serde(rename = "type")]
    pub script_type: Option<String>,
    pub addresses: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum RpcError {
    Network(String),
    RateLimited,
    HttpError { status: u16, body: String },
    Parse(String),
    Rpc { code: i64, message: String },
    EmptyResponse,
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "network error: {msg}"),
            Self::RateLimited => f.write_str("rate limited (429)"),
            Self::HttpError { status, body } => write!(f, "HTTP {status}: {body}"),
            Self::Parse(msg) => write!(f, "parse error: {msg}"),
            Self::Rpc { code, message } => write!(f, "RPC error {code}: {message}"),
            Self::EmptyResponse => f.write_str("empty RPC response"),
        }
    }
}

impl std::error::Error for RpcError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_rpc_client_with_endpoint() {
        let rpc = ZebradRpc::new("http://127.0.0.1:18232");
        assert_eq!(rpc.endpoint, "http://127.0.0.1:18232");
    }

    #[test]
    fn deserializes_verbose_transaction() {
        let json = r#"{
            "txid": "abc123",
            "confirmations": 5,
            "vout": [{
                "value": 0.125,
                "valueZat": 12500000,
                "n": 0,
                "scriptPubKey": {
                    "type": "scripthash",
                    "addresses": ["t2HifwjUj9uyxr9bknR8LFuQbc98c3vkXtu"]
                }
            }]
        }"#;
        let tx: VerboseTransaction =
            serde_json::from_str(json).expect("should deserialize verbose tx");
        assert_eq!(tx.confirmations, Some(5));
        let vout = tx.vout.expect("should have vout");
        assert_eq!(vout.len(), 1);
        assert_eq!(vout[0].value_zat, Some(12500000));
        let spk = vout[0].script_pub_key.as_ref().expect("should have script");
        let addrs = spk.addresses.as_ref().expect("should have addresses");
        assert_eq!(addrs[0], "t2HifwjUj9uyxr9bknR8LFuQbc98c3vkXtu");
    }

    #[test]
    fn deserializes_rpc_error() {
        let json = r#"{"id":1,"jsonrpc":"2.0","error":{"code":-5,"message":"No such mempool or main chain transaction"}}"#;
        let resp: JsonRpcResponse<String> =
            serde_json::from_str(json).expect("should deserialize error");
        let err = resp.into_result().expect_err("should be error");
        assert!(matches!(err, RpcError::Rpc { code: -5, .. }));
    }
}
