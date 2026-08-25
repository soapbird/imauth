import type * as grpc from "@grpc/grpc-js";
import type { AuthEvent, Cookie, CredentialInfo, CredentialSaveResult, Platform } from "./types";
import type {
  AuthStatusResponseWire,
  ConnectionStatusWire,
  CookieListWire,
  NetscapeExportWire,
  ValidationResultWire,
} from "./grpc_wire";

export type GrpcUnaryCallback<Response> = (
  error: grpc.ServiceError | null,
  response?: Response,
) => void;

export type LoginRequest = { readonly platform: Platform };
export type StatusRequest = { readonly sessionId: string };
export type CancelRequest = { readonly sessionId: string };
export type GetCookiesRequest = {
  readonly platform: Platform;
  readonly domains: readonly string[];
};
export type UpdateCookiesRequest = {
  readonly platform: Platform;
  readonly cookies: readonly Cookie[];
};
export type PlatformRequest = { readonly platform: Platform };
export type SaveCredentialsRequest = {
  readonly platform: Platform;
  readonly username: string;
  readonly password: string;
  readonly twofaMethod: string;
};

export interface AuthGrpcClient {
  Login(request: LoginRequest, metadata: grpc.Metadata): grpc.ClientReadableStream<AuthEvent>;
  GetStatus(
    request: StatusRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<AuthStatusResponseWire>,
  ): void;
  Cancel(request: CancelRequest, metadata: grpc.Metadata, callback: GrpcUnaryCallback<void>): void;
  close(): void;
}

export interface SessionGrpcClient {
  GetCookies(
    request: GetCookiesRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<CookieListWire>,
  ): void;
  UpdateCookies(
    request: UpdateCookiesRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<void>,
  ): void;
  ExportNetscape(
    request: PlatformRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<NetscapeExportWire>,
  ): void;
  ValidateSession(
    request: PlatformRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<ValidationResultWire>,
  ): void;
  GetConnectionStatus(
    request: Readonly<Record<string, never>>,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<ConnectionStatusWire>,
  ): void;
  close(): void;
}

export interface CredentialGrpcClient {
  Save(
    request: SaveCredentialsRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<CredentialSaveResult>,
  ): void;
  Get(
    request: PlatformRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<CredentialInfo>,
  ): void;
  Delete(
    request: PlatformRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<void>,
  ): void;
  close(): void;
}

export interface GrpcClients {
  readonly auth: AuthGrpcClient;
  readonly session: SessionGrpcClient;
  readonly credential: CredentialGrpcClient;
}
