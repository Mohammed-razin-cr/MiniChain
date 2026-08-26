const DEFAULT_BASE_URL =
  process.env.NEXT_PUBLIC_MINICHAIN_API_URL || "http://127.0.0.1:9201/api/v1";
export class ApiError extends Error {
  constructor(message, status, code, requestId) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.requestId = requestId;
  }
}
export function createApiClient({ token, baseUrl = DEFAULT_BASE_URL }) {
  async function request(path, options = {}) {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 10000);
    try {
      const response = await fetch(`${baseUrl.replace(/\/$/, "")}${path}`, {
        ...options,
        signal: controller.signal,
        headers: {
          Accept: "application/json",
          ...(options.body ? { "Content-Type": "application/json" } : {}),
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
      });
      const payload = await response.json().catch(() => ({}));
      if (!response.ok)
        throw new ApiError(
          payload?.error?.message || `Node returned HTTP ${response.status}`,
          response.status,
          payload?.error?.code || "HTTP_ERROR",
          response.headers.get("x-request-id"),
        );
      return payload;
    } catch (error) {
      if (error.name === "AbortError")
        throw new ApiError(
          "The node did not respond within 10 seconds.",
          408,
          "TIMEOUT",
        );
      throw error;
    } finally {
      clearTimeout(timeout);
    }
  }
  return {
    get: (path) => request(path),
    post: (path, body) =>
      request(path, {
        method: "POST",
        body: body ? JSON.stringify(body) : undefined,
      }),
    baseUrl,
  };
}
export { DEFAULT_BASE_URL };
