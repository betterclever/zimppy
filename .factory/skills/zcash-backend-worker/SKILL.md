---
name: zcash-backend-worker
description: Builds the Rust Zcash backend, payment verification adapters, memo handling, session accounting, and race-safe state changes.
---

# Zcash Backend Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the work procedure.

## When to Use This Skill

Use this skill for features centered on Rust backend logic: address and key abstractions, memo binding, transparent and shielded verification, remote lightwalletd adapters, session accounting, replay protection, and concurrency-safe state updates.

## Work Procedure

1. Read `mission.md`, `AGENTS.md`, `.factory/services.yaml`, and `.factory/library/zcash.md` before coding.
2. Restate the exact validation assertions your feature fulfills, including any failure-path requirements.
3. Write failing Rust unit or integration tests first for the feature’s core backend behavior.
4. Implement backend logic behind clear interfaces; keep secrets and chain details out of the TypeScript layer except through explicit contracts.
5. Add negative-path tests for mismatches, replays, stale proofs, and state corruption risks where relevant.
6. For session features, prove race safety with concurrent tests or equivalent deterministic state-transition checks.
7. Run targeted `cargo test` first, then broader backend checks from `.factory/services.yaml`.
8. Perform at least one manual end-to-end or backend-driven verification step that shows the app consumed the backend behavior correctly.
9. Confirm that backend logs redact secrets and that no local Zcash infra was started as part of the feature work.

## Example Handoff

```json
{
  "salientSummary": "Implemented the shielded memo-binding and verification adapter in the Rust backend. Valid shielded proofs can now be matched to challenge metadata, while mismatched memo payloads fail cleanly.",
  "whatWasImplemented": "Added Rust modules for shielded receiving-term generation, memo payload encoding and decoding, remote chain-service verification adapters, and negative-path handling for mismatched challenge bindings. Added tests covering valid memo recovery, missing memo rejection, and wrong-recipient rejection.",
  "whatWasLeftUndone": "The TypeScript layer still needs to surface the new backend shielded success path through the shared protected endpoint.",
  "verification": {
    "commandsRun": [
      {
        "command": "cargo test -p zimppy-backend shielded",
        "exitCode": 0,
        "observation": "Shielded backend tests passed for memo binding and mismatch rejection."
      },
      {
        "command": "cargo check --workspace --all-targets",
        "exitCode": 0,
        "observation": "Workspace type checks passed after the new backend interfaces were added."
      },
      {
        "command": "cargo clippy --workspace --all-targets -- -D warnings",
        "exitCode": 0,
        "observation": "No clippy warnings remained in the backend changes."
      }
    ],
    "interactiveChecks": [
      {
        "action": "Ran a backend-level verification sample against a memo-bound shielded payload and inspected the summarized verifier output.",
        "observed": "The verifier matched the expected challenge binding without printing raw memo contents."
      }
    ]
  },
  "tests": {
    "added": [
      {
        "file": "crates/zimppy-backend/tests/shielded_verification.rs",
        "cases": [
          {
            "name": "accepts valid memo-bound shielded proof",
            "verifies": "VAL-SHIELD-002 and VAL-SHIELD-003"
          },
          {
            "name": "rejects missing or mismatched memo binding",
            "verifies": "VAL-SHIELD-004"
          }
        ]
      }
    ]
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- The feature requires unsupported remote chain-service capabilities or a new external dependency not planned in the mission.
- Shielded or session behavior needs a product decision about trust model or settlement semantics.
- Safe replay protection or concurrency guarantees cannot be implemented within the existing storage boundary.
- The only apparent way forward would require starting local Zcash infrastructure.
