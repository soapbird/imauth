export enum Platform {
  UNSPECIFIED = 0,
  INSTAGRAM = 1,
  THREADS = 2,
  NAVER = 3,
}

export enum AuthStatus {
  UNSPECIFIED = 0,
  IDLE = 1,
  LOADING = 2,
  AUTHENTICATING = 3,
  WAITING_FOR_USER = 4,
  CONNECTED = 7,
  FAILED = 8,
}

export interface Cookie {
  name: string;
  value: string;
  domain: string;
  path: string;
  expires: number;
  httpOnly: boolean;
  secure: boolean;
}

export interface AuthEvent {
  status: AuthStatus;
  sessionId: string;
  message: string;
  requiresInput: boolean;
  inputType: string;
  cookies: Cookie[];
  screenshot: Buffer;
  viewerUrl: string;
}

export interface SessionValidation {
  valid: boolean;
  expiresAt: number;
  sessionCookieName: string;
}

export interface CredentialInfo {
  platform: Platform;
  username: string;
  hasPassword: boolean;
  twofaMethod: string;
}
