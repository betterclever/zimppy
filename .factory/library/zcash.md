# Zcash Notes

Protocol- and tooling-specific facts workers should preserve.

**What belongs here:** Zcash-specific implementation constraints, wallet notes, verification assumptions, and chain-service facts relevant to this mission.

---

- Zcash is UTXO-based and does not provide smart-contract escrow.
- Shielded verification requires challenge binding through memo or equivalent recoverable metadata.
- Testnet only for this mission.
- Transparent verification is simpler; shielded verification is the differentiator.
- `lightwalletd` is the planned remote chain service for client-style chain access.
- Session refunds/settlement are off-chain trust-model behavior and must be explicit in code and tests.
- Never assume local node RPC access.
