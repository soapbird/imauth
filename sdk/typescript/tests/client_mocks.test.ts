/**
 * Mock-based unit tests for the TypeScript imauth client.
 *
 * The client constructs three gRPC clients at instantiation (AuthService,
 * SessionService, CredentialService). We replace them with simple stubs so
 * we can assert on the request payloads, exercise both success and error
 * branches of every Promise wrapper without real network activity.
 */

import * as grpc from "@grpc/grpc-js";
import { ImauthClient, Platform, AuthStatus } from "../src";

type Callback<T> = (error: grpc.ServiceError | null, response?: T) => void;
type FakeGrpcClient = Record<string, ReturnType<typeof jest.fn>>;

class TestServiceError extends Error implements grpc.ServiceError {
  readonly code: grpc.status;
  readonly details: string;
  readonly metadata = new grpc.Metadata();

  constructor(message: string, code: grpc.status = grpc.status.UNKNOWN) {
    super(message);
    this.name = "TestServiceError";
    this.code = code;
    this.details = message;
  }
}

function makeStreamingStub() {
  // Login is a server-streaming RPC. The TS client just returns whatever
  // the gRPC stub returns from .Login(), so we hand back a sentinel object
  // and check that the call was forwarded with the right payload.
  const Login = jest.fn(() => ({ kind: "stream", _items: [] }));
  return { Login };
}

function makeUnaryStub<TRequest extends object, TResponse>(
  handler: (request: TRequest) => { readonly err?: grpc.ServiceError; readonly resp?: TResponse },
) {
  // The client now passes per-call metadata (for API-key auth) between the
  // request and the callback, so the stub must accept (req, metadata, cb).
  return jest.fn((request: TRequest, _metadata: grpc.Metadata, callback: Callback<TResponse>) => {
    const { err, resp } = handler(request);
    if (err !== undefined) {
      callback(err);
      return;
    }
    callback(null, resp);
  });
}

function installFakeClients(client: ImauthClient) {
  const authFake: FakeGrpcClient = makeStreamingStub();
  const sessionFake: FakeGrpcClient = {};
  const credentialFake: FakeGrpcClient = {};

  Object.defineProperty(client, "authClient", { value: authFake });
  Object.defineProperty(client, "sessionClient", { value: sessionFake });
  Object.defineProperty(client, "credentialClient", { value: credentialFake });

  return { authFake, sessionFake, credentialFake };
}

describe("ImauthClient (mocked stubs)", () => {
  let client: ImauthClient;

  beforeEach(() => {
    client = new ImauthClient("test:1234");
  });

  afterEach(() => {
    // Skip close — close() touches the real grpc clients we replaced.
  });

  // ---- login (streaming) ------------------------------------------------

  it("login forwards platform to AuthService.Login", () => {
    const { authFake } = installFakeClients(client);
    const stream = client.login(Platform.INSTAGRAM);

    expect(authFake.Login).toHaveBeenCalledTimes(1);
    const callArg = authFake.Login.mock.calls[0][0];
    expect(callArg).toEqual({
      platform: Platform.INSTAGRAM,
    });
    expect(stream).toBeDefined();
  });

  it("login passes the NAVER platform value", () => {
    const { authFake } = installFakeClients(client);
    client.login(Platform.NAVER);
    expect(authFake.Login.mock.calls[0][0].platform).toBe(Platform.NAVER);
  });

  // ---- getCookies ------------------------------------------------------

  it("getStatus exposes the exact public AuthEvent shape", async () => {
    const { authFake } = installFakeClients(client);
    authFake.GetStatus = makeUnaryStub(() => ({
      resp: {
        status: AuthStatus.IDLE,
        sessionId: "session-1",
        message: "idle",
        requiresInput: false,
        inputType: "",
      },
    }));

    const event = await client.getStatus("session-1");

    expect(authFake.GetStatus.mock.calls[0][0]).toEqual({ sessionId: "session-1" });
    expect(event).toEqual({
      status: AuthStatus.IDLE,
      sessionId: "session-1",
      message: "idle",
      requiresInput: false,
      inputType: "",
      cookies: [],
      viewerUrl: "",
    });
  });

  it("cancel forwards the camel-case session ID", async () => {
    const { authFake } = installFakeClients(client);
    authFake.Cancel = makeUnaryStub(() => ({ resp: undefined }));

    await client.cancel("cancel-session-1");

    expect(authFake.Cancel.mock.calls[0][0]).toEqual({ sessionId: "cancel-session-1" });
  });

  it("getCookies returns the cookies array", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.GetCookies = makeUnaryStub(() => ({
      resp: {
        cookies: [
          {
            name: "sessionid",
            value: "abc",
            domain: ".instagram.com",
            path: "/",
            expires: 1_900_000_000,
            httpOnly: true,
            secure: true,
          },
          {
            name: "csrftoken",
            value: "xyz",
            domain: ".instagram.com",
            path: "/",
            expires: 0,
            httpOnly: false,
            secure: true,
          },
        ],
      },
    }));

    const cookies = await client.getCookies(Platform.INSTAGRAM);
    expect(cookies.map((cookie) => cookie.name)).toEqual(["sessionid", "csrftoken"]);
    expect(cookies[0]).toMatchObject({ httpOnly: true, expires: 1_900_000_000 });
    expect(sessionFake.GetCookies.mock.calls[0][0]).toEqual({
      platform: Platform.INSTAGRAM,
      domains: [],
    });
  });

  it("getCookies returns the decoded empty default", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.GetCookies = makeUnaryStub(() => ({ resp: { cookies: [] } }));

    const cookies = await client.getCookies(Platform.THREADS);
    expect(cookies).toEqual([]);
  });

  it("getCookies rejects on gRPC error", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.GetCookies = makeUnaryStub(() => ({ err: new TestServiceError("net") }));
    await expect(client.getCookies(Platform.INSTAGRAM)).rejects.toThrow("net");
  });

  it("updateCookies sends camel-case fields used by proto-loader", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.UpdateCookies = makeUnaryStub(() => ({ resp: { cookies: [] } }));

    await client.updateCookies(Platform.INSTAGRAM, [
      {
        name: "sessionid",
        value: "abc",
        domain: ".instagram.com",
        path: "/",
        expires: 1_900_000_000,
        httpOnly: true,
        secure: true,
      },
    ]);

    expect(sessionFake.UpdateCookies.mock.calls[0][0].cookies[0]).toMatchObject({
      httpOnly: true,
      expires: 1_900_000_000,
    });
  });

  it("validateSessionDetails preserves expiry and cookie name", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.ValidateSession = makeUnaryStub(() => ({
      resp: {
        valid: true,
        expiresAt: 1_900_000_000,
        sessionCookieName: "sessionid",
      },
    }));

    await expect(client.validateSessionDetails(Platform.INSTAGRAM)).resolves.toEqual({
      valid: true,
      expiresAt: 1_900_000_000,
      sessionCookieName: "sessionid",
    });
    await expect(client.validateSession(Platform.INSTAGRAM)).resolves.toBe(true);
  });

  // ---- exportNetscape --------------------------------------------------

  it("exportNetscape resolves with content string", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.ExportNetscape = makeUnaryStub(() => ({
      resp: { content: "# Netscape\n.instagram.com\tTRUE\t/\tTRUE\t0\ts\tv\n" },
    }));

    const content = await client.exportNetscape(Platform.INSTAGRAM);
    expect(content).toContain("Netscape");
    expect(sessionFake.ExportNetscape.mock.calls[0][0]).toEqual({
      platform: Platform.INSTAGRAM,
    });
  });

  it("exportNetscape rejects on gRPC error", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.ExportNetscape = makeUnaryStub(() => ({
      err: new TestServiceError("export failed"),
    }));
    await expect(client.exportNetscape(Platform.INSTAGRAM)).rejects.toThrow("export failed");
  });

  // ---- getConnectionStatus --------------------------------------------

  it("getConnectionStatus returns the platforms map", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.GetConnectionStatus = makeUnaryStub(() => ({
      resp: { platforms: { instagram: true, threads: false } },
    }));

    const status = await client.getConnectionStatus();
    expect(status).toEqual({ instagram: true, threads: false });
  });

  it("getConnectionStatus returns the decoded empty default", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.GetConnectionStatus = makeUnaryStub(() => ({ resp: { platforms: {} } }));
    const status = await client.getConnectionStatus();
    expect(status).toEqual({});
  });

  it("getConnectionStatus rejects on gRPC error", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.GetConnectionStatus = makeUnaryStub(() => ({
      err: new TestServiceError("unreachable"),
    }));
    await expect(client.getConnectionStatus()).rejects.toThrow("unreachable");
  });

  // ---- saveCredentials ------------------------------------------------

  it("saveCredentials serializes all four fields", async () => {
    const { credentialFake } = installFakeClients(client);
    credentialFake.Save = makeUnaryStub(() => ({
      resp: { success: true, platform: Platform.INSTAGRAM, username: "alice" },
    }));

    await expect(
      client.saveCredentials(Platform.INSTAGRAM, "alice", "pw", "totp"),
    ).resolves.toEqual({
      success: true,
      platform: Platform.INSTAGRAM,
      username: "alice",
    });
    expect(credentialFake.Save.mock.calls[0][0]).toEqual({
      platform: Platform.INSTAGRAM,
      username: "alice",
      password: "pw",
      twofaMethod: "totp",
    });
  });

  it("saveCredentials defaults twofaMethod to empty string", async () => {
    const { credentialFake } = installFakeClients(client);
    credentialFake.Save = makeUnaryStub(() => ({
      resp: { success: true, platform: Platform.THREADS, username: "bob" },
    }));

    await client.saveCredentials(Platform.THREADS, "bob", "pw");
    expect(credentialFake.Save.mock.calls[0][0].twofaMethod).toBe("");
  });

  it("saveCredentials rejects on gRPC error", async () => {
    const { credentialFake } = installFakeClients(client);
    credentialFake.Save = makeUnaryStub(() => ({
      err: new TestServiceError("duplicate"),
    }));
    await expect(client.saveCredentials(Platform.INSTAGRAM, "u", "p")).rejects.toThrow("duplicate");
  });
});
