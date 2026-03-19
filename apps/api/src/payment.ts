import { createHmac } from "node:crypto";

import type { ApiRuntimeConfig } from "./config.js";

export const PROTECTED_RESOURCE_PATH = "/protected/resource";

const CHALLENGE_REALM = "zimppy";
const CHALLENGE_METHOD = "zcash-testnet";
const CHALLENGE_INTENT = "charge";
const CHALLENGE_SECRET = "zimppy-local-demo-secret";
const CHALLENGE_TTL_MS = 5 * 60 * 1000;
const PAYMENT_AMOUNT_ZAT = 42_000;
const PAYMENT_ASSET = "ZEC";
const DEMO_RECIPIENT = {
  kind: "transparent_p2pkh",
  value: "tmYd5nFLM8ptuA6A9LTqCVhGfX3Wb5f4K8p",
};

export type ChallengeRequestPayload = {
  challengeId: string;
  resource: string;
  payment: {
    network: string;
    amountZat: number;
    asset: string;
    receiver: {
      kind: string;
      value: string;
    };
  };
  requestBinding: {
    method: string;
    path: string;
  };
  verifier: {
    service: string;
    endpoint: string;
    access: string;
  };
  expiresAt: string;
};

export type PaymentChallenge = {
  scheme: "Payment";
  id: string;
  realm: string;
  method: string;
  intent: string;
  request: ChallengeRequestPayload;
};

export type ProblemDetails = {
  type: string;
  title: string;
  status: number;
  detail: string;
} & Record<string, unknown>;

export type PaymentCredential = {
  challenge: {
    id: string;
    method: string;
    intent: string;
    request: ChallengeRequestPayload;
  };
  source: string;
  payload: unknown;
};

export function createPaymentChallenge(
  config: ApiRuntimeConfig,
  requestMethod: string,
  requestPath: string,
): PaymentChallenge {
  const challengeId = createChallengeId(config, requestMethod, requestPath);

  return {
    scheme: "Payment",
    id: challengeId,
    realm: CHALLENGE_REALM,
    method: CHALLENGE_METHOD,
    intent: CHALLENGE_INTENT,
    request: {
      challengeId,
      resource: requestPath,
      payment: {
        network: config.remoteChainService.network,
        amountZat: PAYMENT_AMOUNT_ZAT,
        asset: PAYMENT_ASSET,
        receiver: DEMO_RECIPIENT,
      },
      requestBinding: {
        method: requestMethod,
        path: requestPath,
      },
      verifier: {
        service: "remote-lightwalletd",
        endpoint: config.remoteChainService.lightwalletd.endpoint,
        access: config.remoteChainService.lightwalletd.access,
      },
      expiresAt: new Date(Date.now() + CHALLENGE_TTL_MS).toISOString(),
    },
  };
}

export function toAuthenticateHeader(challenge: PaymentChallenge): string {
  const encodedRequest = Buffer.from(JSON.stringify(challenge.request), "utf8").toString(
    "base64url",
  );

  return [
    `${challenge.scheme} id="${challenge.id}"`,
    `realm="${challenge.realm}"`,
    `method="${challenge.method}"`,
    `intent="${challenge.intent}"`,
    `request="${encodedRequest}"`,
  ].join(", ");
}

export function parsePaymentCredential(headerValue: string): PaymentCredential {
  if (!headerValue.startsWith("Payment ")) {
    throw new Error("unsupported Payment authorization scheme");
  }

  const encodedCredential = headerValue.slice("Payment ".length).trim();
  if (encodedCredential.length === 0) {
    throw new Error("missing Payment credential payload");
  }

  try {
    const credential = JSON.parse(
      Buffer.from(encodedCredential, "base64url").toString("utf8"),
    ) as Partial<PaymentCredential>;

    if (
      credential.challenge === undefined ||
      credential.challenge === null ||
      typeof credential.challenge !== "object" ||
      credential.challenge.request === undefined ||
      credential.challenge.request === null ||
      typeof credential.challenge.request !== "object" ||
      typeof credential.challenge.id !== "string" ||
      typeof credential.challenge.method !== "string" ||
      typeof credential.challenge.intent !== "string" ||
      typeof credential.source !== "string"
    ) {
      throw new Error("credential shape is invalid");
    }

    return credential as PaymentCredential;
  } catch {
    throw new Error("invalid credential encoding");
  }
}

export function isExpiredCredential(credential: PaymentCredential): boolean {
  const expiresAt = Date.parse(credential.challenge.request.expiresAt);

  return Number.isNaN(expiresAt) || expiresAt <= Date.now();
}

export function isChallengeBoundToRequest(
  credential: PaymentCredential,
  config: ApiRuntimeConfig,
  requestMethod: string,
  requestPath: string,
): boolean {
  const expectedChallengeId = createChallengeId(config, requestMethod, requestPath);

  return (
    credential.challenge.id === expectedChallengeId &&
    credential.challenge.method === CHALLENGE_METHOD &&
    credential.challenge.intent === CHALLENGE_INTENT &&
    credential.challenge.request.challengeId === expectedChallengeId &&
    credential.challenge.request.resource === requestPath &&
    credential.challenge.request.payment.network === config.remoteChainService.network &&
    credential.challenge.request.payment.amountZat === PAYMENT_AMOUNT_ZAT &&
    credential.challenge.request.requestBinding.method === requestMethod &&
    credential.challenge.request.requestBinding.path === requestPath &&
    credential.challenge.request.verifier.endpoint ===
      config.remoteChainService.lightwalletd.endpoint
  );
}

export function problemDetails(
  type: string,
  title: string,
  status: number,
  detail: string,
  extras: Record<string, unknown> = {},
): ProblemDetails {
  return {
    type,
    title,
    status,
    detail,
    ...extras,
  };
}

function createChallengeId(
  config: ApiRuntimeConfig,
  requestMethod: string,
  requestPath: string,
): string {
  return createHmac("sha256", CHALLENGE_SECRET)
    .update(
      [
        CHALLENGE_REALM,
        CHALLENGE_METHOD,
        CHALLENGE_INTENT,
        requestMethod,
        requestPath,
        config.remoteChainService.network,
        String(PAYMENT_AMOUNT_ZAT),
      ].join("|"),
    )
    .digest("base64url");
}
