# zimppy-core

[![crates.io](https://img.shields.io/crates/v/zimppy-core)](https://crates.io/crates/zimppy-core)

Zcash verification engine for [zimppy](https://zimppy.xyz) — the privacy stack for [MPP](https://mpp.dev).

Orchard shielded transaction decryption, memo verification, replay protection, and challenge ID generation.

## Features

- `default` — HTTP server with charge verification
- `shielded` — Zcash Orchard note decryption and verification
- `keygen` — Key generation and derivation utilities

## Usage

```toml
[dependencies]
zimppy-core = { version = "0.3", features = ["shielded"] }
```

## License

MIT
