/**
 * Mock-based unit tests for the TypeScript imauth client.
 *
 * The client constructs three gRPC clients at instantiation (AuthService,
 * SessionService, CredentialService). We replace them with simple stubs so
 * we can assert on the request payloads, exercise both success and error
 * branches of every Promise wrapper, and avoid any real network activity.
 */

import { ImauthClient, Platform, AuthStatus } from "../src";

type Callback<T> = (err: any, response: T | null) => void;

function makeStreamingStub() {
  // Login is a server-streaming RPC. The TS client just returns whatever
  // the gRPC stub returns from .Login(), so we hand back a sentinel object
  // and check that the call was forwarded with the right payload.
  const Login = jest.fn(() => ({ kind: "stream", _items: [] }));
  return { Login };
}

function makeUnaryStub<TResp>(handler: (req: any) => { err?: any; resp?: TResp }) {
  // The client now passes per-call metadata (for API-key auth) between the
  // request and the callback, so the stub must accept (req, metadata, cb).
  return jest.fn((req: any, _metadata: any, cb: Callback<TResp>) => {
    const { err, resp } = handler(req);
    if (err) {
      cb(err, null);
    } else {
      cb(null, resp ?? (null as any));
    }
  });
}

function installFakeClients(client: ImauthClient) {
  const authFake: any = makeStreamingStub();
  const sessionFake: any = {};
  const credentialFake: any = {};

  // @ts-expect-error reach into private state for test isolation
  client.authClient = authFake;
  // @ts-expect-error
  client.sessionClient = sessionFake;
  // @ts-expect-error
  client.credentialClient = credentialFake;

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

  it("getCookies returns [] when response.cookies is missing", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.GetCookies = makeUnaryStub(() => ({ resp: {} as any }));

    const cookies = await client.getCookies(Platform.THREADS);
    expect(cookies).toEqual([]);
  });

  it("getCookies rejects on gRPC error", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.GetCookies = makeUnaryStub(() => ({ err: new Error("net") }));
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
      err: new Error("export failed"),
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

  it("getConnectionStatus returns {} when platforms is missing", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.GetConnectionStatus = makeUnaryStub(() => ({ resp: {} as any }));
    const status = await client.getConnectionStatus();
    expect(status).toEqual({});
  });

  it("getConnectionStatus rejects on gRPC error", async () => {
    const { sessionFake } = installFakeClients(client);
    sessionFake.GetConnectionStatus = makeUnaryStub(() => ({
      err: new Error("unreachable"),
    }));
    await expect(client.getConnectionStatus()).rejects.toThrow("unreachable");
  });

  // ---- saveCredentials ------------------------------------------------

  it("saveCredentials serializes all four fields", async () => {
    const { credentialFake } = installFakeClients(client);
    credentialFake.Save = makeUnaryStub(() => ({ resp: {} }));

    await client.saveCredentials(Platform.INSTAGRAM, "alice", "pw", "totp");
    expect(credentialFake.Save.mock.calls[0][0]).toEqual({
      platform: Platform.INSTAGRAM,
      username: "alice",
      password: "pw",
      twofaMethod: "totp",
    });
  });

  it("saveCredentials defaults twofaMethod to empty string", async () => {
    const { credentialFake } = installFakeClients(client);
    credentialFake.Save = makeUnaryStub(() => ({ resp: {} }));

    await client.saveCredentials(Platform.THREADS, "bob", "pw");
    expect(credentialFake.Save.mock.calls[0][0].twofaMethod).toBe("");
  });

  it("saveCredentials rejects on gRPC error", async () => {
    const { credentialFake } = installFakeClients(client);
    credentialFake.Save = makeUnaryStub(() => ({
      err: new Error("duplicate"),
    }));
    await expect(client.saveCredentials(Platform.INSTAGRAM, "u", "p")).rejects.toThrow("duplicate");
  });
});

describe("Enum + module surface", () => {
  it("Platform enum is dense at the expected integer values", () => {
    expect(Platform.UNSPECIFIED).toBe(0);
    expect(Platform.INSTAGRAM).toBe(1);
    expect(Platform.THREADS).toBe(2);
    expect(Platform.NAVER).toBe(3);
  });

  it("AuthStatus covers every state", () => {
    expect(AuthStatus.IDLE).toBe(1);
    expect(AuthStatus.LOADING).toBe(2);
    expect(AuthStatus.AUTHENTICATING).toBe(3);
    expect(AuthStatus.WAITING_FOR_USER).toBe(4);
    expect(AuthStatus.CONNECTED).toBe(7);
    expect(AuthStatus.FAILED).toBe(8);
  });
});
