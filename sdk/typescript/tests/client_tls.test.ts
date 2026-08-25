import * as grpc from "@grpc/grpc-js";

type JestMock = ReturnType<typeof jest.fn>;

declare global {
  var imauthTlsMocks: {
    readonly clientClose: JestMock;
    readonly clientConstructor: JestMock;
  };
}

jest.mock("@grpc/grpc-js", () => {
  const actual = jest.requireActual<typeof import("@grpc/grpc-js")>("@grpc/grpc-js");
  const clientClose = jest.fn();
  const clientConstructor = jest.fn(() => ({
    close: clientClose,
    makeServerStreamRequest: jest.fn(),
    makeUnaryRequest: jest.fn(),
  }));
  globalThis.imauthTlsMocks = {
    clientClose,
    clientConstructor,
  };

  return {
    ...actual,
    Client: clientConstructor,
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
    expect(globalThis.imauthTlsMocks.clientConstructor).toHaveBeenCalledTimes(3);
    expect(globalThis.imauthTlsMocks.clientConstructor).toHaveBeenNthCalledWith(
      1,
      "localhost:6100",
      insecureCredentials,
      undefined,
    );
    expect(globalThis.imauthTlsMocks.clientConstructor).toHaveBeenNthCalledWith(
      2,
      "localhost:6100",
      insecureCredentials,
      undefined,
    );
    expect(globalThis.imauthTlsMocks.clientConstructor).toHaveBeenNthCalledWith(
      3,
      "localhost:6100",
      insecureCredentials,
      undefined,
    );

    client.close();
    expect(globalThis.imauthTlsMocks.clientClose).toHaveBeenCalledTimes(3);
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
    expect(globalThis.imauthTlsMocks.clientConstructor).toHaveBeenCalledTimes(3);
    for (const invocation of globalThis.imauthTlsMocks.clientConstructor.mock.calls) {
      expect(invocation).toEqual(["imauth.internal:6100", tlsCredentials, channelOptions]);
    }

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
    expect(globalThis.imauthTlsMocks.clientConstructor).toHaveBeenCalledTimes(3);
    for (const invocation of globalThis.imauthTlsMocks.clientConstructor.mock.calls) {
      expect(invocation).toEqual(["imauth.internal:6100", tlsCredentials, undefined]);
    }

    client.close();
  });
});
