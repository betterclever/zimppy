---
name: protocol-worker
description: Builds the TypeScript MPP surface, API contracts, receipts, problem details, and local app wiring.
---

# Protocol Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the work procedure.

## When to Use This Skill

Use this skill for features that primarily change the TypeScript HTTP/API surface, MPP challenge and receipt handling, protected-route behavior, response contracts, local storage wiring from the app layer, or cross-mode API consistency.

## Work Procedure

1. Read `mission.md`, `AGENTS.md`, `.factory/services.yaml`, and the relevant library notes before touching code.
2. Identify the exact assertions the feature fulfills and restate them in your notes before changing files.
3. Write or extend failing tests first for the HTTP/API behavior you are changing.
4. Implement only enough code to make those tests pass while preserving the split boundary: TypeScript should call Rust-backed services for Zcash-specific operations rather than reimplementing them.
5. Run targeted tests after each change; keep iteration fast.
6. Manually verify the local HTTP surface with `curl` for the primary path and at least one negative path.
7. Run the relevant validators from `.factory/services.yaml` before finishing: typecheck, lint, targeted tests, and any feature-specific integration checks.
8. Confirm that logs do not expose credentials, viewing keys, memo contents, or session bearer secrets.
9. Stop any local processes you started and report exact verification evidence in the handoff.

## Example Handoff

```json
{
  "salientSummary": "Implemented the initial protected-route MPP surface for Zimppy. Unpaid requests now return a Payment challenge, malformed credentials return RFC 9457 problem details, and the local demo starts without any local Zcash infra.",
  "whatWasImplemented": "Added TypeScript API route wiring for a protected resource, challenge generation, problem-detail error responses, and app-layer config loading for the remote chain-service path. Added tests for unpaid 402 behavior, decoded challenge terms, and malformed credential failures.",
  "whatWasLeftUndone": "Successful payment authorization is still pending the transparent verification feature.",
  "verification": {
    "commandsRun": [
      {
        "command": "npm run test -- protected-route",
        "exitCode": 0,
        "observation": "Protected-route tests passed, covering unpaid and malformed-credential paths."
      },
      {
        "command": "npm run typecheck",
        "exitCode": 0,
        "observation": "TypeScript types passed after the route and response-contract changes."
      },
      {
        "command": "npm run lint",
        "exitCode": 0,
        "observation": "Lint passed with no remaining diagnostics."
      }
    ],
    "interactiveChecks": [
      {
        "action": "Started the local API, curled the protected endpoint without credentials, and decoded the challenge header.",
        "observed": "Received 402 Payment Required with a Payment challenge containing the expected method, intent, amount, and testnet metadata."
      },
      {
        "action": "Curled the same endpoint with a malformed payment credential.",
        "observed": "Received a problem-detail error response and no protected resource body."
      }
    ]
  },
  "tests": {
    "added": [
      {
        "file": "apps/api/test/protected-route.test.ts",
        "cases": [
          {
            "name": "returns 402 and Payment challenge when unpaid",
            "verifies": "VAL-API-001 and VAL-API-002"
          },
          {
            "name": "returns problem details for malformed credential",
            "verifies": "VAL-API-004"
          }
        ]
      }
    ]
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- The feature requires Zcash-specific logic to move into TypeScript instead of Rust.
- Remote chain access assumptions conflict with the mission boundaries.
- The required API contract is ambiguous across payment modes.
- Existing validators or startup commands in `.factory/services.yaml` are fundamentally incompatible with the feature and need orchestration changes.
