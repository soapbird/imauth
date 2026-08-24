export enum Platform {
  UNSPECIFIED = 0,
  INSTAGRAM = 1,
  THREADS = 2,
  NAVER = 3,
  NOVELPIA = 4,
  MUNPIA = 5,
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
  readonly name: string;
  readonly value: string;
  readonly domain: string;
  readonly path: string;
  readonly expires: number;
  readonly httpOnly: boolean;
  readonly secure: boolean;
}

export interface AuthEvent {
  readonly status: AuthStatus;
  readonly sessionId: string;
  readonly message: string;
  readonly requiresInput: boolean;
  readonly inputType: string;
  readonly cookies: readonly Cookie[];
  readonly viewerUrl: string;
}

export interface SessionValidation {
  readonly valid: boolean;
  readonly expiresAt: number;
  readonly sessionCookieName: string;
}

export interface CredentialInfo {
  readonly platform: Platform;
  readonly username: string;
  readonly hasPassword: boolean;
  readonly twofaMethod: string;
}

export interface CredentialSaveResult {
  readonly success: boolean;
  readonly platform: Platform;
  readonly username: string;
}
