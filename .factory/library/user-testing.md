# User Testing

Testing surfaces, validation setup notes, and concurrency guidance.

**What belongs here:** user-testing surface details, runtime setup notes, flow grouping guidance, and resource-cost observations.

---

## Validation Surface

- Primary validation surface: local HTTP API on port `3180`
- Supporting local service: Rust backend on port `3181`
- Remote dependency path: local SSH-tunneled access to remote `lightwalletd` on port `3184`
- Validators should use `curl` and scripted CLI flows; browser tooling is not required.
- Validators must confirm that no local Zcash infra is started during setup or testing.

## Validation Concurrency

- Machine profile observed during planning: 8 CPU cores, 16 GB RAM
- Planned max concurrent validators for the HTTP surface: 3
- Rationale: local API plus Rust backend are moderate-weight, while heavy chain infrastructure stays remote; reserve headroom for Rust builds, Node processes, SSH tunnels, and test helpers.

## Flow Coverage Expectations

- Transparent one-time charge: unpaid request, challenge decode, payment, retry, receipt
- Shielded one-time charge: unpaid request, challenge decode, memo-bound payment, retry, receipt
- Session flow: open, use, insufficient balance, top-up, close, post-close rejection
- Remote outage: fail-closed verification path
