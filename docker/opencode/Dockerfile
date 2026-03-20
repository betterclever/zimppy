FROM golang:1.24-bookworm AS builder

RUN go install github.com/opencode-ai/opencode@latest

FROM node:22-bookworm-slim

# Install opencode
COPY --from=builder /go/bin/opencode /usr/local/bin/opencode

# Install project deps
WORKDIR /app
COPY package.json package-lock.json ./
COPY packages/ packages/
COPY apps/mcp-server/ apps/mcp-server/
COPY apps/demo/ apps/demo/
COPY config/ config/
COPY crates/zimppy-napi/ crates/zimppy-napi/
RUN npm install --ignore-scripts 2>/dev/null || npm install

# OpenCode config
COPY docker/opencode/opencode.json /app/opencode.json

ENV TERM=xterm-256color

ENTRYPOINT ["opencode"]
