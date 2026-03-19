# Architecture

Architectural decisions, cross-cutting patterns, and implementation boundaries.

**What belongs here:** system boundaries, protocol decisions, storage choices, and cross-service contracts.

---

- Preserve a split architecture:
  - TypeScript handles HTTP 402 / MPP surface and app wiring.
  - Rust handles Zcash-specific logic and chain-facing verification behavior.
- The local user-facing surface is an HTTP API only.
- Transparent, shielded, and session flows should share one protected-resource contract at the HTTP layer.
- Remote chain verification must fail closed.
- Shielded verification should use memo-bound request matching and remote lightwalletd-backed detection.
- Session support uses off-chain accounting and an explicit trust model for refunds/settlement.
