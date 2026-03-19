import http from "node:http";

import { loadApiRuntimeConfig } from "./config.js";

export function createApiServer() {
  const config = loadApiRuntimeConfig();

  return http.createServer((request, response) => {
    if (request.url === "/health") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(
        JSON.stringify({
          service: "zimppy-api",
          status: "ok",
          ports: config.localApp.ports,
          remoteChainService: {
            network: config.remoteChainService.network,
            endpoint: config.remoteChainService.lightwalletd.endpoint,
          },
          storage: {
            stateDirectory: config.localApp.storage.stateDirectory,
          },
        }),
      );
      return;
    }

    response.writeHead(404, { "content-type": "application/json" });
    response.end(
      JSON.stringify({
        type: "about:blank",
        title: "Not Found",
        status: 404,
        detail: `No route for ${request.method ?? "GET"} ${request.url ?? "/"}`,
      }),
    );
  });
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const config = loadApiRuntimeConfig();
  const server = createApiServer();

  server.listen(config.apiPort, "127.0.0.1", () => {
    console.log(
      `zimppy-api listening on http://127.0.0.1:${config.apiPort} with backend port ${config.backendPort}`,
    );
  });
}
