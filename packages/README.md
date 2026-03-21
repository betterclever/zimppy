# packages/

TypeScript SDK and CLI for Zimppy.

## zimppy-ts

TypeScript SDK implementing the MPP client protocol. Wraps `zimppy-napi` for native Zcash verification.

- MPPX protocol client (challenge-response payment flow)
- Session management (open, bearer, topUp, close)
- SSE streaming consumer with per-token payment tracking

## zimppy-cli

CLI tool for making paid HTTP requests with automatic Zcash payment handling.

```bash
npx zimppy wallet login          # Set up wallet
npx zimppy wallet whoami         # Show address + balance
npx zimppy wallet services       # Discover paid services
npx zimppy request <URL>         # Make request with auto-pay
npx zimppy request --dry-run <URL>  # Preview without paying
npx zimppy session close         # Close session, get refund
```

Supports charge (one-time), session (prepaid), and stream (pay-per-token) payment intents. Service discovery via `/.well-known/payment`.
