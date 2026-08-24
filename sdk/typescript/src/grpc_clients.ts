import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import * as path from "node:path";
import type { AuthEvent } from "./types";
import { CredentialClient } from "./credential_grpc_client";
import type {
  AuthGrpcClient,
  CancelRequest,
  GetCookiesRequest,
  GrpcClients,
  GrpcUnaryCallback,
  LoginRequest,
  PlatformRequest,
  SessionGrpcClient,
  StatusRequest,
  UpdateCookiesRequest,
} from "./grpc_contracts";
import {
  parseAuthEvent,
  parseAuthStatusResponse,
  parseConnectionStatus,
  parseCookieList,
  parseNetscapeExport,
  parseValidationResult,
  type AuthStatusResponseWire,
  type ConnectionStatusWire,
  type CookieListWire,
  type NetscapeExportWire,
  type ValidationResultWire,
} from "./grpc_wire";

const PROTO_ROOT = path.join(__dirname, "../proto");

const packageDefinition = protoLoader.loadSync(
  [
    path.join(PROTO_ROOT, "imauth/v1/common.proto"),
    path.join(PROTO_ROOT, "imauth/v1/auth.proto"),
    path.join(PROTO_ROOT, "imauth/v1/session.proto"),
    path.join(PROTO_ROOT, "imauth/v1/credential.proto"),
  ],
  {
    keepCase: false,
    longs: Number,
    defaults: true,
    oneofs: true,
    includeDirs: [PROTO_ROOT],
  },
);

function methodDefinition(
  serviceName: string,
  methodName: string,
): protoLoader.MethodDefinition<object, object> {
  const service = packageDefinition[serviceName];
  if (service === undefined || "format" in service) {
    throw new Error(`Missing gRPC service definition: ${serviceName}`);
  }
  const method = service[methodName];
  if (method === undefined) {
    throw new Error(`Missing gRPC method definition: ${serviceName}.${methodName}`);
  }
  return method;
}

function requestSerializer<Request extends object>(
  method: protoLoader.MethodDefinition<object, object>,
): (request: Request) => Buffer {
  return (request) => method.requestSerialize(request);
}

function responseDeserializer<Response>(
  method: protoLoader.MethodDefinition<object, object>,
  parse: (value: unknown) => Response,
): (bytes: Buffer) => Response {
  return (bytes) => parse(method.responseDeserialize(bytes));
}

const loginMethod = methodDefinition("imauth.v1.AuthService", "Login");
const getStatusMethod = methodDefinition("imauth.v1.AuthService", "GetStatus");
const cancelMethod = methodDefinition("imauth.v1.AuthService", "Cancel");
const getCookiesMethod = methodDefinition("imauth.v1.SessionService", "GetCookies");
const updateCookiesMethod = methodDefinition("imauth.v1.SessionService", "UpdateCookies");
const exportNetscapeMethod = methodDefinition("imauth.v1.SessionService", "ExportNetscape");
const validateSessionMethod = methodDefinition("imauth.v1.SessionService", "ValidateSession");
const getConnectionStatusMethod = methodDefinition(
  "imauth.v1.SessionService",
  "GetConnectionStatus",
);
const saveCredentialsMethod = methodDefinition("imauth.v1.CredentialService", "Save");
const getCredentialsMethod = methodDefinition("imauth.v1.CredentialService", "Get");
const deleteCredentialsMethod = methodDefinition("imauth.v1.CredentialService", "Delete");

class AuthClient implements AuthGrpcClient {
  constructor(private readonly client: grpc.Client) {}

  Login(request: LoginRequest, metadata: grpc.Metadata): grpc.ClientReadableStream<AuthEvent> {
    return this.client.makeServerStreamRequest(
      loginMethod.path,
      requestSerializer<LoginRequest>(loginMethod),
      responseDeserializer(loginMethod, parseAuthEvent),
      request,
      metadata,
    );
  }

  GetStatus(
    request: StatusRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<AuthStatusResponseWire>,
  ): void {
    this.client.makeUnaryRequest(
      getStatusMethod.path,
      requestSerializer<StatusRequest>(getStatusMethod),
      responseDeserializer(getStatusMethod, parseAuthStatusResponse),
      request,
      metadata,
      callback,
    );
  }

  Cancel(request: CancelRequest, metadata: grpc.Metadata, callback: GrpcUnaryCallback<void>): void {
    this.client.makeUnaryRequest(
      cancelMethod.path,
      requestSerializer<CancelRequest>(cancelMethod),
      responseDeserializer(cancelMethod, () => undefined),
      request,
      metadata,
      callback,
    );
  }

  close(): void {
    this.client.close();
  }
}

class SessionClient implements SessionGrpcClient {
  constructor(private readonly client: grpc.Client) {}

  GetCookies(
    request: GetCookiesRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<CookieListWire>,
  ): void {
    this.client.makeUnaryRequest(
      getCookiesMethod.path,
      requestSerializer<GetCookiesRequest>(getCookiesMethod),
      responseDeserializer(getCookiesMethod, parseCookieList),
      request,
      metadata,
      callback,
    );
  }

  UpdateCookies(
    request: UpdateCookiesRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<void>,
  ): void {
    this.client.makeUnaryRequest(
      updateCookiesMethod.path,
      requestSerializer<UpdateCookiesRequest>(updateCookiesMethod),
      responseDeserializer(updateCookiesMethod, () => undefined),
      request,
      metadata,
      callback,
    );
  }

  ExportNetscape(
    request: PlatformRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<NetscapeExportWire>,
  ): void {
    this.client.makeUnaryRequest(
      exportNetscapeMethod.path,
      requestSerializer<PlatformRequest>(exportNetscapeMethod),
      responseDeserializer(exportNetscapeMethod, parseNetscapeExport),
      request,
      metadata,
      callback,
    );
  }

  ValidateSession(
    request: PlatformRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<ValidationResultWire>,
  ): void {
    this.client.makeUnaryRequest(
      validateSessionMethod.path,
      requestSerializer<PlatformRequest>(validateSessionMethod),
      responseDeserializer(validateSessionMethod, parseValidationResult),
      request,
      metadata,
      callback,
    );
  }

  GetConnectionStatus(
    request: Readonly<Record<string, never>>,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<ConnectionStatusWire>,
  ): void {
    this.client.makeUnaryRequest(
      getConnectionStatusMethod.path,
      requestSerializer<Readonly<Record<string, never>>>(getConnectionStatusMethod),
      responseDeserializer(getConnectionStatusMethod, parseConnectionStatus),
      request,
      metadata,
      callback,
    );
  }

  close(): void {
    this.client.close();
  }
}

export function createGrpcClients(
  serverAddress: string,
  credentials: grpc.ChannelCredentials,
  options?: grpc.ClientOptions,
): GrpcClients {
  return {
    auth: new AuthClient(new grpc.Client(serverAddress, credentials, options)),
    session: new SessionClient(new grpc.Client(serverAddress, credentials, options)),
    credential: new CredentialClient(new grpc.Client(serverAddress, credentials, options), {
      save: saveCredentialsMethod,
      get: getCredentialsMethod,
      delete: deleteCredentialsMethod,
    }),
  };
}
