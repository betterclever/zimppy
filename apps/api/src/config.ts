import { existsSync, mkdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export type LocalAppConfig = {
  projectName: string;
  ports: {
    api: number;
    backend: number;
    testHelper: number;
    integrationHarness: number;
    lightwalletdTunnel: number;
  };
  storage: {
    stateDirectory: string;
    sqliteFile: string;
  };
  services: {
    apiBaseUrl: string;
    backendBaseUrl: string;
  };
};

export type RemoteChainServiceConfig = {
  network: string;
  lightwalletd: {
    access: string;
    host: string;
    port: number;
    endpoint: string;
  };
  upstream: {
    hostAlias: string;
    remotePort: number;
  };
};

export type ApiRuntimeConfig = {
  localApp: LocalAppConfig;
  remoteChainService: RemoteChainServiceConfig;
  apiPort: number;
  backendPort: number;
  stateDirectory: string;
};

function resolveRepoRoot(fromImportMetaUrl: string): string {
  let currentDirectory = path.dirname(fileURLToPath(fromImportMetaUrl));

  while (true) {
    const hasLocalAppConfig = existsSync(path.join(currentDirectory, "config/local-app.json"));
    const hasRemoteChainConfig = existsSync(
      path.join(currentDirectory, "config/remote-chain-service.json"),
    );

    if (hasLocalAppConfig && hasRemoteChainConfig) {
      return currentDirectory;
    }

    const parentDirectory = path.dirname(currentDirectory);
    if (parentDirectory === currentDirectory) {
      throw new Error(
        `Unable to resolve repo root from ${fromImportMetaUrl}: missing split runtime config files`,
      );
    }

    currentDirectory = parentDirectory;
  }
}

const repoRoot = resolveRepoRoot(import.meta.url);

function readJsonFile<T>(relativePath: string): T {
  const filePath = path.join(repoRoot, relativePath);
  return JSON.parse(readFileSync(filePath, "utf8")) as T;
}

export function loadApiRuntimeConfig(): ApiRuntimeConfig {
  const localApp = readJsonFile<LocalAppConfig>("config/local-app.json");
  const remoteChainService = readJsonFile<RemoteChainServiceConfig>(
    "config/remote-chain-service.json",
  );
  const stateDirectory = path.join(repoRoot, localApp.storage.stateDirectory);

  mkdirSync(stateDirectory, { recursive: true });

  return {
    localApp,
    remoteChainService,
    apiPort: Number(process.env.PORT ?? localApp.ports.api),
    backendPort: localApp.ports.backend,
    stateDirectory,
  };
}
