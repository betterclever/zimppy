import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import path from "node:path";

import { loadApiRuntimeConfig } from "../src/config.js";

const repoRoot = fileURLToPath(new URL("../../../", import.meta.url));

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
  assert.equal(config.remoteChainService.lightwalletd.access, "ssh-tunnel");
  assert.equal(config.remoteChainService.lightwalletd.host, "127.0.0.1");
  assert.equal(config.remoteChainService.lightwalletd.port, 3184);
  assert.equal(config.remoteChainService.lightwalletd.endpoint, "http://127.0.0.1:3184");
  assert.equal(config.remoteChainService.upstream.hostAlias, "bettervps");
  assert.equal(config.remoteChainService.upstream.remotePort, 9067);
  assert.match(config.localApp.storage.sqliteFile, /\.local\/state\/zimppy\/app\.sqlite3$/);
  assert.equal(existsSync(config.stateDirectory), true);
});

test("built api artifact resolves split config files from the repository root", async () => {
  execFileSync("npm", ["run", "build"], {
    cwd: repoRoot,
    stdio: "pipe",
  });

  const builtConfigModule = await import(
    pathToFileURL(path.join(repoRoot, "dist/apps/api/src/config.js")).href
  );
  const config = builtConfigModule.loadApiRuntimeConfig() as ReturnType<typeof loadApiRuntimeConfig>;

  assert.equal(config.localApp.projectName, "zimppy");
  assert.equal(config.remoteChainService.lightwalletd.endpoint, "http://127.0.0.1:3184");
  assert.equal(config.stateDirectory, path.join(repoRoot, ".local/state/zimppy"));
  assert.equal(existsSync(path.join(repoRoot, "config/local-app.json")), true);
  assert.equal(existsSync(path.join(repoRoot, "config/remote-chain-service.json")), true);
});
