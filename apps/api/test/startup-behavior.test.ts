import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../../../", import.meta.url));

test("service startup remains remote-only and SSH-tunnel-friendly", () => {
  const manifest = readFileSync(path.join(repoRoot, ".factory/services.yaml"), "utf8");
  const startCommands = manifest
    .split("\n")
    .filter((line) => line.trimStart().startsWith("start:"));

  assert.match(
    manifest,
    /start: ssh -f -N -L 3184:127\.0\.0\.1:9067 bettervps/,
  );
  assert.match(manifest, /depends_on: \[lightwalletd-tunnel\]/);

  for (const line of startCommands) {
    assert.doesNotMatch(line, /\b(zebrad|zcashd|lightwalletd)\b/);
  }
});
