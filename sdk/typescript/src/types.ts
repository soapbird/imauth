export enum Platform {
  UNSPECIFIED = 0,
  INSTAGRAM = 1,
  THREADS = 2,
}

export enum AuthStatus {
  UNSPECIFIED = 0,
  IDLE = 1,
  LOADING = 2,
  AUTHENTICATING = 3,
  NEEDS_CREDS = 4,
  NEEDS_2FA = 5,
  NEEDS_CAPTCHA = 6,
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
  message: string;
  requiresInput: boolean;
  inputType: string;
  cookies: Cookie[];
  screenshot: Buffer;
}

export interface CredentialInfo {
  platform: Platform;
  username: string;
  hasPassword: boolean;
  twofaMethod: string;
}
