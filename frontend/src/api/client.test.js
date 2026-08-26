import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError, createApiClient } from "./client";

afterEach(() => vi.unstubAllGlobals());

describe("MiniChain API client", () => {
  it("sends a bearer token without persisting it", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValue(
        new Response(JSON.stringify({ identity: "viewer", role: "viewer" }), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
      );
    vi.stubGlobal("fetch", fetch);
    const client = createApiClient({
      token: "secret",
      baseUrl: "http://node/api/v1",
    });
    await expect(client.get("/auth/whoami")).resolves.toEqual({
      identity: "viewer",
      role: "viewer",
    });
    expect(fetch.mock.calls[0][0]).toBe("http://node/api/v1/auth/whoami");
    expect(fetch.mock.calls[0][1].headers.Authorization).toBe("Bearer secret");
  });

  it("preserves structured errors and request IDs", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValue(
          new Response(
            JSON.stringify({
              error: { code: "NOT_FOUND", message: "Missing block" },
            }),
            { status: 404, headers: { "x-request-id": "request-7" } },
          ),
        ),
    );
    const error = await createApiClient({
      token: "secret",
      baseUrl: "http://node/api/v1",
    })
      .get("/blocks/99")
      .catch((value) => value);
    expect(error).toBeInstanceOf(ApiError);
    expect(error).toMatchObject({
      status: 404,
      code: "NOT_FOUND",
      requestId: "request-7",
      message: "Missing block",
    });
  });
});
