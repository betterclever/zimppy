import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";

import { loadApiRuntimeConfig } from "../src/config.js";

test("loads split local and remote configuration with reserved ports", () => {
  const config = loadApiRuntimeConfig();

  assert.equal(config.localApp.projectName, "zimppy");
  assert.deepEqual(config.localApp.ports, {
    api: 3180,
    backend: 3181,
    testHelper: 3182,
    integrationHarness: 3183,
    lightwalletdTunnel: 3184,
  });
  assert.equal(config.remoteChainService.network, "testnet");
  assert.equal(config.remoteChainService.lightwalletd.port, 3184);
  assert.match(config.localApp.storage.sqliteFile, /\.local\/state\/zimppy\/app\.sqlite3$/);
  assert.equal(existsSync(config.stateDirectory), true);
});
