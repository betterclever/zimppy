## Remote lightwalletd connectivity foundation

- Approved local access path is an SSH tunnel on `127.0.0.1:3184` forwarding to `bettervps:9067`.
- Local startup stays remote-only: `.factory/services.yaml` starts the SSH tunnel, Rust backend, and API, and does not launch `zebrad`, `lightwalletd`, or `zcashd` locally.
- Run `cargo run --bin zimppy-backend -- check-remote-lightwalletd` (or `commands.check_remote_lightwalletd` from `.factory/services.yaml`) to fail fast on tunnel/connectivity issues before payment verification flows.
- Health responses expose the remote-only chain path metadata (`access`, `endpoint`, `upstreamHostAlias`, `upstreamPort`) so validators can confirm the configured chain dependency without inspecting code.
