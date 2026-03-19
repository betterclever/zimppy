import http from "node:http";

import { loadApiRuntimeConfig } from "./config.js";
import {
  createPaymentChallenge,
  isChallengeBoundToRequest,
  isExpiredCredential,
  parsePaymentCredential,
  problemDetails,
  PROTECTED_RESOURCE_PATH,
  toAuthenticateHeader,
} from "./payment.js";

const PROBLEM_DETAILS_CONTENT_TYPE = "application/problem+json";

export function createApiServer() {
  const config = loadApiRuntimeConfig();

  return http.createServer((request, response) => {
    const requestMethod = request.method ?? "GET";
    const requestPath = new URL(request.url ?? "/", "http://127.0.0.1").pathname;

    if (request.url === "/health") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(
        JSON.stringify({
          service: "zimppy-api",
          status: "ok",
          ports: config.localApp.ports,
          remoteChainService: {
            network: config.remoteChainService.network,
            access: config.remoteChainService.lightwalletd.access,
            endpoint: config.remoteChainService.lightwalletd.endpoint,
            upstreamHostAlias: config.remoteChainService.upstream.hostAlias,
            upstreamPort: config.remoteChainService.upstream.remotePort,
          },
          storage: {
            stateDirectory: config.localApp.storage.stateDirectory,
          },
        }),
      );
      return;
    }

    if (requestMethod === "GET" && requestPath === PROTECTED_RESOURCE_PATH) {
      const authorization = request.headers.authorization;

      if (authorization === undefined) {
        const challenge = createPaymentChallenge(config, requestMethod, requestPath);

        response.writeHead(402, {
          "content-type": PROBLEM_DETAILS_CONTENT_TYPE,
          "www-authenticate": toAuthenticateHeader(challenge),
        });
        response.end(
          JSON.stringify(
            problemDetails(
              "https://zimppy.local/problems/payment-required",
              "Payment Required",
              402,
              "Payment is required before this resource can be accessed.",
              { challengeId: challenge.id },
            ),
          ),
        );
        return;
      }

      try {
        const credential = parsePaymentCredential(authorization);

        if (isExpiredCredential(credential)) {
          response.writeHead(401, { "content-type": PROBLEM_DETAILS_CONTENT_TYPE });
          response.end(
            JSON.stringify(
              problemDetails(
                "https://zimppy.local/problems/payment-credential-expired",
                "Expired payment credential",
                401,
                "The supplied Payment credential has expired.",
              ),
            ),
          );
          return;
        }

        if (!isChallengeBoundToRequest(credential, config, requestMethod, requestPath)) {
          response.writeHead(400, { "content-type": PROBLEM_DETAILS_CONTENT_TYPE });
          response.end(
            JSON.stringify(
              problemDetails(
                "https://zimppy.local/problems/payment-credential-mismatch",
                "Payment credential mismatch",
                400,
                "The supplied Payment credential does not match this protected request.",
              ),
            ),
          );
          return;
        }

        response.writeHead(501, { "content-type": PROBLEM_DETAILS_CONTENT_TYPE });
        response.end(
          JSON.stringify(
            problemDetails(
              "https://zimppy.local/problems/payment-verification-not-implemented",
              "Payment verification not implemented",
              501,
              "Zcash payment verification is not implemented on this route yet.",
            ),
          ),
        );
        return;
      } catch {
        response.writeHead(400, { "content-type": PROBLEM_DETAILS_CONTENT_TYPE });
        response.end(
          JSON.stringify(
            problemDetails(
              "https://zimppy.local/problems/invalid-payment-credential",
              "Invalid payment credential",
              400,
              "The Payment credential could not be decoded.",
            ),
          ),
        );
        return;
      }
    }

    response.writeHead(404, { "content-type": PROBLEM_DETAILS_CONTENT_TYPE });
    response.end(
      JSON.stringify({
        type: "about:blank",
        title: "Not Found",
        status: 404,
        detail: `No route for ${requestMethod} ${request.url ?? "/"}`,
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
