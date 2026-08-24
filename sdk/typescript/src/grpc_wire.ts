import type {
  AuthEvent,
  AuthStatus,
  Cookie,
  CredentialInfo,
  CredentialSaveResult,
  Platform,
} from "./types";
import { AuthStatus as AuthStatusValue, Platform as PlatformValue } from "./types";

export class GrpcResponseError extends Error {
  constructor(readonly field: string) {
    super(`Invalid gRPC response field: ${field}`);
    this.name = "GrpcResponseError";
  }
}

export interface AuthStatusResponseWire {
  readonly status: AuthStatus;
  readonly sessionId: string;
  readonly message: string;
  readonly requiresInput: boolean;
  readonly inputType: string;
}

export interface CookieListWire {
  readonly cookies: readonly Cookie[];
}

export interface NetscapeExportWire {
  readonly content: string;
}

export interface ValidationResultWire {
  readonly valid: boolean;
  readonly expiresAt: number;
  readonly sessionCookieName: string;
}

export interface ConnectionStatusWire {
  readonly platforms: Readonly<Record<string, boolean>>;
}

function readRecord(value: unknown, field: string): Readonly<Record<string, unknown>> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new GrpcResponseError(field);
  }
  return Object.fromEntries(Object.entries(value));
}

function readString(record: Readonly<Record<string, unknown>>, field: string): string {
  const value = record[field];
  if (typeof value !== "string") {
    throw new GrpcResponseError(field);
  }
  return value;
}

function readBoolean(record: Readonly<Record<string, unknown>>, field: string): boolean {
  const value = record[field];
  if (typeof value !== "boolean") {
    throw new GrpcResponseError(field);
  }
  return value;
}

function readNumber(record: Readonly<Record<string, unknown>>, field: string): number {
  const value = record[field];
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new GrpcResponseError(field);
  }
  return value;
}

function readAuthStatus(record: Readonly<Record<string, unknown>>, field: string): AuthStatus {
  const value = readNumber(record, field);
  switch (value) {
    case AuthStatusValue.UNSPECIFIED:
      return AuthStatusValue.UNSPECIFIED;
    case AuthStatusValue.IDLE:
      return AuthStatusValue.IDLE;
    case AuthStatusValue.LOADING:
      return AuthStatusValue.LOADING;
    case AuthStatusValue.AUTHENTICATING:
      return AuthStatusValue.AUTHENTICATING;
    case AuthStatusValue.WAITING_FOR_USER:
      return AuthStatusValue.WAITING_FOR_USER;
    case AuthStatusValue.CONNECTED:
      return AuthStatusValue.CONNECTED;
    case AuthStatusValue.FAILED:
      return AuthStatusValue.FAILED;
    default:
      throw new GrpcResponseError(field);
  }
}

function readPlatform(record: Readonly<Record<string, unknown>>, field: string): Platform {
  const value = readNumber(record, field);
  switch (value) {
    case PlatformValue.UNSPECIFIED:
      return PlatformValue.UNSPECIFIED;
    case PlatformValue.INSTAGRAM:
      return PlatformValue.INSTAGRAM;
    case PlatformValue.THREADS:
      return PlatformValue.THREADS;
    case PlatformValue.NAVER:
      return PlatformValue.NAVER;
    default:
      throw new GrpcResponseError(field);
  }
}

function readCookie(value: unknown): Cookie {
  const record = readRecord(value, "cookies[]");
  return {
    name: readString(record, "name"),
    value: readString(record, "value"),
    domain: readString(record, "domain"),
    path: readString(record, "path"),
    expires: readNumber(record, "expires"),
    httpOnly: readBoolean(record, "httpOnly"),
    secure: readBoolean(record, "secure"),
  };
}

function readCookies(record: Readonly<Record<string, unknown>>): readonly Cookie[] {
  const value = record.cookies;
  if (!Array.isArray(value)) {
    throw new GrpcResponseError("cookies");
  }
  return value.map(readCookie);
}

export function parseAuthEvent(value: unknown): AuthEvent {
  const record = readRecord(value, "AuthEvent");
  return {
    status: readAuthStatus(record, "status"),
    sessionId: readString(record, "sessionId"),
    message: readString(record, "message"),
    requiresInput: readBoolean(record, "requiresInput"),
    inputType: readString(record, "inputType"),
    cookies: readCookies(record),
    viewerUrl: readString(record, "viewerUrl"),
  };
}

export function parseAuthStatusResponse(value: unknown): AuthStatusResponseWire {
  const record = readRecord(value, "AuthStatusResponse");
  return {
    status: readAuthStatus(record, "status"),
    sessionId: readString(record, "sessionId"),
    message: readString(record, "message"),
    requiresInput: readBoolean(record, "requiresInput"),
    inputType: readString(record, "inputType"),
  };
}

export function parseCookieList(value: unknown): CookieListWire {
  return { cookies: readCookies(readRecord(value, "CookieList")) };
}

export function parseNetscapeExport(value: unknown): NetscapeExportWire {
  const record = readRecord(value, "NetscapeExport");
  return { content: readString(record, "content") };
}

export function parseValidationResult(value: unknown): ValidationResultWire {
  const record = readRecord(value, "ValidationResult");
  return {
    valid: readBoolean(record, "valid"),
    expiresAt: readNumber(record, "expiresAt"),
    sessionCookieName: readString(record, "sessionCookieName"),
  };
}

export function parseConnectionStatus(value: unknown): ConnectionStatusWire {
  const record = readRecord(value, "ConnectionStatusMap");
  const platforms = readRecord(record.platforms, "platforms");
  const entries: [string, boolean][] = [];
  for (const [platform, connected] of Object.entries(platforms)) {
    if (typeof connected !== "boolean") {
      throw new GrpcResponseError(`platforms.${platform}`);
    }
    entries.push([platform, connected]);
  }
  return { platforms: Object.fromEntries(entries) };
}

export function parseCredentialInfo(value: unknown): CredentialInfo {
  const record = readRecord(value, "CredentialInfo");
  return {
    platform: readPlatform(record, "platform"),
    username: readString(record, "username"),
    hasPassword: readBoolean(record, "hasPassword"),
    twofaMethod: readString(record, "twofaMethod"),
  };
}

export function parseCredentialSaveResult(value: unknown): CredentialSaveResult {
  const record = readRecord(value, "CredentialResponse");
  return {
    success: readBoolean(record, "success"),
    platform: readPlatform(record, "platform"),
    username: readString(record, "username"),
  };
}
