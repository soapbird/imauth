import * as grpc from "@grpc/grpc-js";

type JestMock = ReturnType<typeof jest.fn>;

declare global {
  var imauthTlsMocks: {
    readonly authService: JestMock;
    readonly closeClient: JestMock;
    readonly credentialService: JestMock;
    readonly sessionService: JestMock;
  };
}

jest.mock("@grpc/grpc-js", () => {
  const actual = jest.requireActual<typeof import("@grpc/grpc-js")>("@grpc/grpc-js");
  const authService = jest.fn();
  const closeClient = jest.fn();
  const credentialService = jest.fn();
  const sessionService = jest.fn();
  globalThis.imauthTlsMocks = {
    authService,
    closeClient,
    credentialService,
    sessionService,
  };

  return {
    ...actual,
    closeClient,
    loadPackageDefinition: jest.fn(() => ({
      imauth: {
        v1: {
          AuthService: authService,
          CredentialService: credentialService,
          SessionService: sessionService,
        },
      },
    })),
  };
});

import { ImauthClient } from "../src";

describe("ImauthClient transport credentials", () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it("uses one insecure credential object for every stub by default", () => {
    // Given: a client without TLS options.
    const insecureCredentials = grpc.credentials.createInsecure();
    const createInsecure = jest
      .spyOn(grpc.credentials, "createInsecure")
      .mockReturnValue(insecureCredentials);

    // When: the client is constructed.
    const client = new ImauthClient("localhost:6100");

    // Then: every stub receives the one insecure credential object.
    expect(createInsecure).toHaveBeenCalledTimes(1);
    expect(globalThis.imauthTlsMocks.authService).toHaveBeenCalledWith(
      "localhost:6100",
      insecureCredentials,
    );
    expect(globalThis.imauthTlsMocks.sessionService).toHaveBeenCalledWith(
      "localhost:6100",
      insecureCredentials,
    );
    expect(globalThis.imauthTlsMocks.credentialService).toHaveBeenCalledWith(
      "localhost:6100",
      insecureCredentials,
    );

    client.close();
  });

  it("uses one TLS credential object and the configured server name for every stub", () => {
    // Given: a PEM root certificate and TLS server name.
    const rootCert = "test root certificate";
    const serverName = "imauth.internal";
    const tlsCredentials = grpc.credentials.createSsl(Buffer.from(rootCert));
    const createSsl = jest.spyOn(grpc.credentials, "createSsl").mockReturnValue(tlsCredentials);
    const channelOptions = {
      "grpc.ssl_target_name_override": serverName,
    };

    // When: the client is constructed with TLS options.
    const client = new ImauthClient("imauth.internal:6100", {
      rootCert,
      serverName,
    });

    // Then: every stub shares the TLS credentials and server-name override.
    expect(createSsl).toHaveBeenCalledWith(Buffer.from(rootCert));
    expect(globalThis.imauthTlsMocks.authService).toHaveBeenCalledWith(
      "imauth.internal:6100",
      tlsCredentials,
      channelOptions,
    );
    expect(globalThis.imauthTlsMocks.sessionService).toHaveBeenCalledWith(
      "imauth.internal:6100",
      tlsCredentials,
      channelOptions,
    );
    expect(globalThis.imauthTlsMocks.credentialService).toHaveBeenCalledWith(
      "imauth.internal:6100",
      tlsCredentials,
      channelOptions,
    );

    client.close();
  });

  it("enables TLS without a server-name override", () => {
    // Given: a PEM root certificate without an override name.
    const rootCert = "test root certificate";
    const tlsCredentials = grpc.credentials.createSsl(Buffer.from(rootCert));
    const createSsl = jest.spyOn(grpc.credentials, "createSsl").mockReturnValue(tlsCredentials);

    // When: the client is constructed with only the root certificate.
    const client = new ImauthClient("imauth.internal:6100", { rootCert });

    // Then: TLS is enabled without adding a target-name override.
    expect(createSsl).toHaveBeenCalledWith(Buffer.from(rootCert));
    expect(globalThis.imauthTlsMocks.authService).toHaveBeenCalledWith(
      "imauth.internal:6100",
      tlsCredentials,
    );
    expect(globalThis.imauthTlsMocks.sessionService).toHaveBeenCalledWith(
      "imauth.internal:6100",
      tlsCredentials,
    );
    expect(globalThis.imauthTlsMocks.credentialService).toHaveBeenCalledWith(
      "imauth.internal:6100",
      tlsCredentials,
    );

    client.close();
  });
});
