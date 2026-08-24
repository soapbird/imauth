import * as grpc from "@grpc/grpc-js";
import { createGrpcClients } from "./grpc_clients";
import type { AuthGrpcClient, CredentialGrpcClient, SessionGrpcClient } from "./grpc_contracts";
import type {
  AuthEvent,
  Cookie,
  CredentialInfo,
  CredentialSaveResult,
  Platform,
  SessionValidation,
} from "./types";

class MissingGrpcResponseError extends Error {
  constructor(readonly method: string) {
    super(`gRPC method returned no response: ${method}`);
    this.name = "MissingGrpcResponseError";
  }
}

export interface ImauthClientOptions {
  /** Bearer API key to attach as `authorization: Bearer <key>` on every call. */
  readonly apiKey?: string;
  /** PEM-encoded root certificate material that enables TLS. */
  readonly rootCert?: string | Buffer;
  /** TLS server name used to verify the server certificate. */
  readonly serverName?: string;
}

export class ImauthClient {
  private readonly authClient: AuthGrpcClient;
  private readonly sessionClient: SessionGrpcClient;
  private readonly credentialClient: CredentialGrpcClient;
  private readonly apiKey?: string;

  constructor(serverAddress: string = "localhost:6100", options: ImauthClientOptions = {}) {
    this.apiKey = options.apiKey;
    const credentials =
      options.rootCert === undefined
        ? grpc.credentials.createInsecure()
        : grpc.credentials.createSsl(Buffer.from(options.rootCert));
    const channelOptions =
      options.serverName === undefined
        ? undefined
        : { "grpc.ssl_target_name_override": options.serverName };
    const clients = createGrpcClients(serverAddress, credentials, channelOptions);
    this.authClient = clients.auth;
    this.sessionClient = clients.session;
    this.credentialClient = clients.credential;
  }

  /** Build per-call metadata including the bearer API key (if set). */
  private buildMetadata(): grpc.Metadata {
    const meta = new grpc.Metadata();
    if (this.apiKey) {
      meta.set("authorization", `Bearer ${this.apiKey}`);
    }
    return meta;
  }

  // --- auth ---------------------------------------------------------------

  login(platform: Platform): grpc.ClientReadableStream<AuthEvent> {
    return this.authClient.Login({ platform }, this.buildMetadata());
  }

  getStatus(sessionId: string): Promise<AuthEvent | null> {
    return new Promise((resolve, reject) => {
      this.authClient.GetStatus({ sessionId }, this.buildMetadata(), (err, response) => {
        if (err !== null) {
          if (err.code === grpc.status.NOT_FOUND) resolve(null);
          else reject(err);
          return;
        }
        if (response === undefined) {
          reject(new MissingGrpcResponseError("AuthService.GetStatus"));
          return;
        }
        resolve({
          status: response.status,
          sessionId: response.sessionId,
          message: response.message,
          requiresInput: response.requiresInput,
          inputType: response.inputType,
          cookies: [],
          viewerUrl: "",
        });
      });
    });
  }

  cancel(sessionId: string): Promise<void> {
    return new Promise((resolve, reject) => {
      this.authClient.Cancel({ sessionId }, this.buildMetadata(), (err) => {
        if (err !== null && err.code !== grpc.status.NOT_FOUND) reject(err);
        else resolve();
      });
    });
  }

  // --- session ------------------------------------------------------------

  getCookies(platform: Platform, domains: readonly string[] = []): Promise<readonly Cookie[]> {
    return new Promise((resolve, reject) => {
      this.sessionClient.GetCookies(
        { platform, domains },
        this.buildMetadata(),
        (err, response) => {
          if (err !== null) {
            reject(err);
            return;
          }
          if (response === undefined) {
            reject(new MissingGrpcResponseError("SessionService.GetCookies"));
            return;
          }
          resolve(response.cookies);
        },
      );
    });
  }

  updateCookies(platform: Platform, cookies: readonly Cookie[]): Promise<void> {
    return new Promise((resolve, reject) => {
      this.sessionClient.UpdateCookies({ platform, cookies }, this.buildMetadata(), (err) => {
        if (err !== null) reject(err);
        else resolve();
      });
    });
  }

  exportNetscape(platform: Platform): Promise<string> {
    return new Promise((resolve, reject) => {
      this.sessionClient.ExportNetscape({ platform }, this.buildMetadata(), (err, response) => {
        if (err !== null) {
          reject(err);
          return;
        }
        if (response === undefined) {
          reject(new MissingGrpcResponseError("SessionService.ExportNetscape"));
          return;
        }
        resolve(response.content);
      });
    });
  }

  validateSession(platform: Platform): Promise<boolean> {
    return this.validateSessionDetails(platform).then((result) => result.valid);
  }

  validateSessionDetails(platform: Platform): Promise<SessionValidation> {
    return new Promise((resolve, reject) => {
      this.sessionClient.ValidateSession({ platform }, this.buildMetadata(), (err, response) => {
        if (err !== null) {
          reject(err);
          return;
        }
        if (response === undefined) {
          reject(new MissingGrpcResponseError("SessionService.ValidateSession"));
          return;
        }
        resolve(response);
      });
    });
  }

  getConnectionStatus(): Promise<Readonly<Record<string, boolean>>> {
    return new Promise((resolve, reject) => {
      this.sessionClient.GetConnectionStatus({}, this.buildMetadata(), (err, response) => {
        if (err !== null) {
          reject(err);
          return;
        }
        if (response === undefined) {
          reject(new MissingGrpcResponseError("SessionService.GetConnectionStatus"));
          return;
        }
        resolve(response.platforms);
      });
    });
  }

  // --- credentials --------------------------------------------------------

  saveCredentials(
    platform: Platform,
    username: string,
    password: string,
    twofaMethod?: string,
  ): Promise<CredentialSaveResult> {
    return new Promise((resolve, reject) => {
      this.credentialClient.Save(
        {
          platform,
          username,
          password,
          twofaMethod: twofaMethod || "",
        },
        this.buildMetadata(),
        (err, response) => {
          if (err !== null) {
            reject(err);
            return;
          }
          if (response === undefined) {
            reject(new MissingGrpcResponseError("CredentialService.Save"));
            return;
          }
          resolve(response);
        },
      );
    });
  }

  getCredentials(platform: Platform): Promise<CredentialInfo | null> {
    return new Promise((resolve, reject) => {
      this.credentialClient.Get({ platform }, this.buildMetadata(), (err, response) => {
        if (err !== null) {
          if (err.code === grpc.status.NOT_FOUND) resolve(null);
          else reject(err);
          return;
        }
        if (response === undefined) {
          reject(new MissingGrpcResponseError("CredentialService.Get"));
          return;
        }
        resolve(response);
      });
    });
  }

  deleteCredentials(platform: Platform): Promise<boolean> {
    return new Promise((resolve, reject) => {
      this.credentialClient.Delete({ platform }, this.buildMetadata(), (err) => {
        if (err !== null) {
          if (err.code === grpc.status.NOT_FOUND) resolve(false);
          else reject(err);
        } else resolve(true);
      });
    });
  }

  close(): void {
    this.authClient.close();
    this.sessionClient.close();
    this.credentialClient.close();
  }
}
