# demo/

CLI-based demos using real Zcash testnet transactions. Each launches a tmux session with server (left pane) and client (right pane).

## Prerequisites

- Zcash wallet with testnet funds (`npx zimppy wallet login`)
- `tmux` installed
- Rust server built (`cargo build --workspace`)

## Available Demos

### charge/ — One-time Payment

```bash
bash demo/charge/run.sh
```

Flow: `402 Payment Required` -> shielded ZEC payment -> verified -> `200 OK`

### session/ — Prepaid Session

```bash
bash demo/session/run.sh
```

Flow: deposit -> open session -> 3 instant bearer requests -> close with refund

### stream/ — SSE Streaming

```bash
bash demo/stream/run.sh
```

Flow: deposit -> open session -> pay-per-word fortune streaming -> close with refund

### ai-summarizer/ — AI Document Summary

```bash
bash demo/ai-summarizer/run.sh
```

Flow: AI server with streaming summarization, pay-per-token via SSE. Requires VibeProxy running on `localhost:8317`.

### opencode/ — AI Agent Demo

Interactive demo using OpenCode agent with VibeProxy for AI-powered tool use with Zcash micropayments.
