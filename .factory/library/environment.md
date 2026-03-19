# Environment

Environment variables, external dependencies, and setup notes.

**What belongs here:** required env vars, remote service endpoints, SSH tunnel assumptions, storage paths, and external dependency notes.
**What does NOT belong here:** service ports and start commands; those belong in `.factory/services.yaml`.

---

- Local machine must not run Zcash infrastructure.
- Remote host `bettervps` already runs a syncing testnet `zebrad` session.
- Planned chain-service dependency is remote `lightwalletd`.
- Preferred local access path is an SSH tunnel to the remote service.
- Mission scope is testnet only.
- Favor SQLite or file-backed local persistence for receipts, replay protection, and session state.
