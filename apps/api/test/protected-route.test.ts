import assert from "node:assert/strict";
import http from "node:http";
import test from "node:test";

import { createApiServer } from "../src/server.js";

type HttpResponse = {
  statusCode: number;
  headers: http.IncomingHttpHeaders;
  bodyText: string;
  bodyJson: unknown;
};

async function request(
  port: number,
  path: string,
  headers: http.OutgoingHttpHeaders = {},
): Promise<HttpResponse> {
  return await new Promise((resolve, reject) => {
    const request = http.get({ host: "127.0.0.1", port, path, headers }, (response) => {
      const chunks: Buffer[] = [];

      response.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
      response.on("end", () => {
        const bodyText = Buffer.concat(chunks).toString("utf8");
        resolve({
          statusCode: response.statusCode ?? 0,
          headers: response.headers,
          bodyText,
          bodyJson: JSON.parse(bodyText),
        });
      });
    });

    request.on("error", reject);
  });
}

function decodeChallengeHeader(value: string) {
  const scheme = "Payment ";
  assert.equal(value.startsWith(scheme), true, "challenge should use Payment scheme");

  const parameters = new Map<string, string>();
  for (const part of value.slice(scheme.length).split(", ")) {
    const equalsAt = part.indexOf("=");
    assert.notEqual(equalsAt, -1, `expected key=value pair in ${part}`);

    const key = part.slice(0, equalsAt);
    const rawValue = part.slice(equalsAt + 1);
    parameters.set(key, rawValue.replace(/^"|"$/g, ""));
  }

  const encodedRequest = parameters.get("request");
  assert.notEqual(encodedRequest, undefined, "request parameter should be present");

  return {
    id: parameters.get("id"),
    realm: parameters.get("realm"),
    method: parameters.get("method"),
    intent: parameters.get("intent"),
    request: JSON.parse(Buffer.from(encodedRequest!, "base64url").toString("utf8")) as {
      challengeId: string;
      resource: string;
      payment: {
        network: string;
        amountZat: number;
        asset: string;
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
    },
  };
}

function encodeCredential(payload: unknown) {
  return Buffer.from(JSON.stringify(payload), "utf8").toString("base64url");
}

test("returns 402 Payment challenge bound to the protected request when unpaid", async (t) => {
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

  const response = await request(address.port, "/protected/resource");

  assert.equal(response.statusCode, 402);
  assert.equal(response.headers["content-type"], "application/json");
  assert.equal(typeof response.headers["www-authenticate"], "string");

  const challenge = decodeChallengeHeader(response.headers["www-authenticate"] as string);
  assert.equal(challenge.realm, "zimppy");
  assert.equal(challenge.method, "zcash-testnet");
  assert.equal(challenge.intent, "charge");
  assert.equal(challenge.id, challenge.request.challengeId);
  assert.equal(challenge.request.resource, "/protected/resource");
  assert.equal(challenge.request.payment.network, "testnet");
  assert.equal(challenge.request.payment.amountZat, 42_000);
  assert.equal(challenge.request.payment.asset, "ZEC");
  assert.deepEqual(challenge.request.requestBinding, {
    method: "GET",
    path: "/protected/resource",
  });
  assert.deepEqual(challenge.request.verifier, {
    service: "remote-lightwalletd",
    endpoint: "http://127.0.0.1:3184",
    access: "ssh-tunnel",
  });
  assert.equal(typeof challenge.request.expiresAt, "string");

  assert.deepEqual(response.bodyJson, {
    type: "https://zimppy.local/problems/payment-required",
    title: "Payment Required",
    status: 402,
    detail: "Payment is required before this resource can be accessed.",
    challengeId: challenge.id,
  });
});

test("returns RFC 9457 problem details for malformed payment credentials", async (t) => {
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

  const response = await request(address.port, "/protected/resource", {
    authorization: "Payment definitely-not-base64url",
  });

  assert.equal(response.statusCode, 400);
  assert.equal(response.headers["www-authenticate"], undefined);
  assert.deepEqual(response.bodyJson, {
    type: "https://zimppy.local/problems/invalid-payment-credential",
    title: "Invalid payment credential",
    status: 400,
    detail: "The Payment credential could not be decoded.",
  });
});

test("returns RFC 9457 problem details for expired payment credentials", async (t) => {
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

  const expiredCredential = encodeCredential({
    challenge: {
      id: "expired-challenge",
      method: "zcash-testnet",
      intent: "charge",
      request: {
        challengeId: "expired-challenge",
        resource: "/protected/resource",
        payment: {
          network: "testnet",
          amountZat: 42_000,
          asset: "ZEC",
        },
        requestBinding: {
          method: "GET",
          path: "/protected/resource",
        },
        verifier: {
          service: "remote-lightwalletd",
          endpoint: "http://127.0.0.1:3184",
          access: "ssh-tunnel",
        },
        expiresAt: "2020-01-01T00:00:00.000Z",
      },
    },
    source: "did:key:z6Mkhx-demo",
    payload: {
      txid: "00".repeat(32),
    },
  });

  const response = await request(address.port, "/protected/resource", {
    authorization: `Payment ${expiredCredential}`,
  });

  assert.equal(response.statusCode, 401);
  assert.deepEqual(response.bodyJson, {
    type: "https://zimppy.local/problems/payment-credential-expired",
    title: "Expired payment credential",
    status: 401,
    detail: "The supplied Payment credential has expired.",
  });
});
