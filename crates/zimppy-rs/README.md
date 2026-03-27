# zimppy-rs

Rust SDK for [zimppy](https://zimppy.xyz) — the privacy stack for [MPP](https://mpp.dev).

Implements `ChargeMethod`, `ChargeChallenger`, `PaymentProvider`, and axum extractors for Zcash shielded payments.

## Server (axum)

```rust
use mpp::server::axum::*;
use zimppy_rs::ZcashChallenger;

struct Price;
impl ChargeConfig for Price {
    fn amount() -> &'static str { "42000" }
}

async fn handler(charge: MppCharge<Price>) -> WithReceipt<Json<Value>> {
    WithReceipt { receipt: charge.receipt, body: Json(data) }
}
```

## Client

```rust
use mpp::client::Fetch;
use zimppy_rs::ZcashPaymentProvider;

let provider = ZcashPaymentProvider::new(wallet_config, &rpc);
let resp = client.get(url).send_with_payment(&provider).await?;
```

## Usage

```toml
[dependencies]
zimppy-rs = { git = "https://github.com/betterclever/zimppy" }
```

> Not yet on crates.io — blocked by zingolib upstream.

## License

MIT
