---
name: integration-worker
description: Wires the TypeScript and Rust layers together, manages remote-service connectivity, demo flows, and end-to-end validation hardening.
---

# Integration Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the work procedure.

## When to Use This Skill

Use this skill for features that stitch together multiple layers: remote lightwalletd connectivity, shared protected-endpoint behavior across payment modes, end-to-end transparent or shielded payment flows, demo scripts, outage handling, and validation-hardening work.

## Work Procedure

1. Read `mission.md`, `AGENTS.md`, `.factory/services.yaml`, and `.factory/library/user-testing.md` before editing anything.
2. List the exact assertions your feature fulfills and identify which services and commands will prove them.
3. Add failing integration or end-to-end tests first; for demo-script features, add automation that can be rerun non-interactively.
4. Implement the smallest wiring changes needed across TypeScript, Rust, config, and scripts to satisfy the assertions.
5. Start only the local services defined in `.factory/services.yaml`; never start local Zcash infra.
6. Verify the primary happy path manually with `curl` or scripted CLI flow, then verify at least one important failure path.
7. Run the relevant validation commands from `.factory/services.yaml` and any targeted integration suite added by the feature.
8. Confirm the remote chain dependency is actually being exercised where the feature requires it; if connectivity fails, return to the orchestrator rather than faking success.
9. Stop all local processes and record precise commands, observations, and any remaining blockers in the handoff.

## Example Handoff

```json
{
  "salientSummary": "Completed the transparent end-to-end payment flow through the shared protected endpoint. The local API now uses the configured remote lightwalletd access path, valid transparent payments return the resource plus receipt, and replay remains blocked by the backend.",
  "whatWasImplemented": "Connected the TypeScript route and Rust verification backend through the remote lightwalletd tunnel configuration, added transparent payment retry handling, and created an end-to-end integration test plus a curl-driven smoke script for the happy path.",
  "whatWasLeftUndone": "Shielded and session flows remain pending and still need equivalent integration coverage.",
  "verification": {
    "commandsRun": [
      {
        "command": "npm run test -- transparent-e2e",
        "exitCode": 0,
        "observation": "Transparent end-to-end tests passed against the local API and backend."
      },
      {
        "command": "cargo test -p zimppy-backend transparent_verification",
        "exitCode": 0,
        "observation": "Backend transparent verification checks passed for the same flow."
      },
      {
        "command": "npm run typecheck && npm run lint",
        "exitCode": 0,
        "observation": "App-layer validators passed after the integration wiring changes."
      }
    ],
    "interactiveChecks": [
      {
        "action": "Started the SSH tunnel, backend, and API; curled the protected endpoint; fulfilled the transparent payment; retried the same request.",
        "observed": "Initial request returned 402, the paid retry returned 200 with protected content and a Payment-Receipt header."
      },
      {
        "action": "Disabled the remote chain-service path and retried verification.",
        "observed": "The API failed closed with a non-success response and did not return the protected resource."
      }
    ]
  },
  "tests": {
    "added": [
      {
        "file": "apps/api/test/transparent_e2e.test.ts",
        "cases": [
          {
            "name": "transparent payment succeeds through remote chain-service path",
            "verifies": "VAL-TCHAR-002 and VAL-CROSS-006"
          },
          {
            "name": "remote outage fails closed",
            "verifies": "VAL-CROSS-003"
          }
        ]
      }
    ]
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- A required remote service or access path is unavailable and cannot be restored locally.
- The integration requires a new mission boundary, such as running local Zcash infrastructure.
- Multiple worker-owned subsystems need a contract change rather than a local fix.
- Full validation cannot be completed because the environment lacks a required remote prerequisite.
