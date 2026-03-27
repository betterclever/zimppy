# zimppy

[![npm](https://img.shields.io/npm/v/zimppy)](https://www.npmjs.com/package/zimppy)

CLI for [zimppy](https://zimppy.xyz) — the privacy stack for [MPP](https://mpp.dev).

Agent-native wallet. Full MPP spec support. One command to pay any 402 endpoint.

## Usage

```bash
npx zimppy wallet create              # generate keys, show seed phrase
npx zimppy wallet whoami              # address, balance, network
npx zimppy request <url>              # auto 402 -> pay -> retry
npx zimppy wallet send <addr> 42000   # shielded transfer
npx zimppy wallet use work            # switch wallet identity
```

## Install

```bash
npm install -g zimppy
```

## License

MIT
