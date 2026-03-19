import assert from "node:assert/strict";
import http from "node:http";
import test from "node:test";

import { createApiServer } from "../src/server.js";

async function requestJson(port: number, path: string) {
  return await new Promise<{ statusCode: number; body: unknown }>((resolve, reject) => {
    const request = http.get({ host: "127.0.0.1", port, path }, (response) => {
      const chunks: Buffer[] = [];

      response.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
      response.on("end", () => {
        const bodyText = Buffer.concat(chunks).toString("utf8");
        resolve({
          statusCode: response.statusCode ?? 0,
          body: JSON.parse(bodyText),
        });
      });
    });

    request.on("error", reject);
  });
}

test("health endpoint reports skeleton service configuration", async (t) => {
  const server = createApiServer();

  await new Promise<void>((resolve) => {
    server.listen(0, "127.0.0.1", resolve);
  });

  t.after(() => {
    server.close();
  });

  const address = server.address();
  if (address === null || typeof address === "string") {
    throw new TypeError("Expected server to bind to a TCP port");
  }

  const response = await requestJson(address.port, "/health");

  assert.equal(response.statusCode, 200);
  assert.deepEqual(response.body, {
    service: "zimppy-api",
    status: "ok",
    ports: {
      api: 3180,
      backend: 3181,
      testHelper: 3182,
      integrationHarness: 3183,
      lightwalletdTunnel: 3184,
    },
    remoteChainService: {
      network: "testnet",
      endpoint: "http://127.0.0.1:3184",
    },
    storage: {
      stateDirectory: ".local/state/zimppy",
    },
  });
});
