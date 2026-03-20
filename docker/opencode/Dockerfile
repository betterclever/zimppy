FROM ghcr.io/anomalyco/opencode

USER root

# Install Node.js on Alpine
RUN apk add --no-cache nodejs npm

# Copy our MCP server and deps
WORKDIR /app
COPY package.json package-lock.json ./
COPY packages/ packages/
COPY apps/mcp-server/ apps/mcp-server/
COPY config/ config/
COPY crates/zimppy-napi/ crates/zimppy-napi/
COPY SKILL.md ./
RUN npm install --ignore-scripts 2>/dev/null || npm install

# OpenCode config
RUN mkdir -p /root/.config/opencode
COPY docker/opencode/opencode.json /root/.config/opencode/opencode.json

WORKDIR /workspace
ENTRYPOINT ["opencode"]
