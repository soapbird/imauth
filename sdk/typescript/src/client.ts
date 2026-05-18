import * as grpc from "@grpc/grpc-js";
import * as protoLoader from "@grpc/proto-loader";
import * as path from "path";
import { AuthEvent, Cookie, CredentialInfo, Platform } from "./types";

const PROTO_ROOT = path.join(__dirname, "../../../proto");

const packageDefinition = protoLoader.loadSync(
  [
    path.join(PROTO_ROOT, "imauth/v1/common.proto"),
    path.join(PROTO_ROOT, "imauth/v1/auth.proto"),
    path.join(PROTO_ROOT, "imauth/v1/session.proto"),
    path.join(PROTO_ROOT, "imauth/v1/credential.proto"),
  ],
  {
    keepCase: true,
    longs: String,
    enums: String,
    defaults: true,
    oneofs: true,
    includeDirs: [PROTO_ROOT],
  }
);

const proto = grpc.loadPackageDefinition(packageDefinition) as any;

export interface ImauthClientOptions {
  /** Bearer API key to attach as `authorization: Bearer <key>` on every call. */
  apiKey?: string;
}

export class ImauthClient {
  private authClient: any;
  private sessionClient: any;
  private credentialClient: any;
  private apiKey?: string;

  constructor(
    serverAddress: string = "localhost:50051",
    options: ImauthClientOptions = {}
  ) {
    this.apiKey = options.apiKey;
    const credentials = grpc.credentials.createInsecure();
    this.authClient = new proto.imauth.v1.AuthService(
      serverAddress,
      credentials
    );
    this.sessionClient = new proto.imauth.v1.SessionService(
      serverAddress,
      credentials
    );
    this.credentialClient = new proto.imauth.v1.CredentialService(
      serverAddress,
      credentials
    );
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

  login(platform: Platform): grpc.ClientReadableStream<any> {
    return this.authClient.Login(
      { platform },
      this.buildMetadata()
    );
  }

  getStatus(sessionId: string): Promise<any> {
    return new Promise((resolve, reject) => {
      this.authClient.GetStatus(
        { session_id: sessionId },
        this.buildMetadata(),
        (err: any, response: any) => {
          if (err) {
            if (err.code === grpc.status.NOT_FOUND) resolve(null);
            else reject(err);
          } else resolve(response);
        }
      );
    });
  }

  cancel(sessionId: string): Promise<void> {
    return new Promise((resolve, reject) => {
      this.authClient.Cancel(
        { session_id: sessionId },
        this.buildMetadata(),
        (err: any) => {
          if (err && err.code !== grpc.status.NOT_FOUND) reject(err);
          else resolve();
        }
      );
    });
  }

  // --- session ------------------------------------------------------------

  getCookies(platform: Platform, domains: string[] = []): Promise<Cookie[]> {
    return new Promise((resolve, reject) => {
      this.sessionClient.GetCookies(
        { platform, domains },
        this.buildMetadata(),
        (err: any, response: any) => {
          if (err) reject(err);
          else resolve(response.cookies || []);
        }
      );
    });
  }

  updateCookies(platform: Platform, cookies: Cookie[]): Promise<void> {
    const wireCookies = cookies.map((c) => ({
      name: c.name,
      value: c.value,
      domain: c.domain,
      path: c.path,
      expires: c.expires,
      http_only: c.httpOnly,
      secure: c.secure,
    }));
    return new Promise((resolve, reject) => {
      this.sessionClient.UpdateCookies(
        { platform, cookies: wireCookies },
        this.buildMetadata(),
        (err: any) => {
          if (err) reject(err);
          else resolve();
        }
      );
    });
  }

  exportNetscape(platform: Platform): Promise<string> {
    return new Promise((resolve, reject) => {
      this.sessionClient.ExportNetscape(
        { platform },
        this.buildMetadata(),
        (err: any, response: any) => {
          if (err) reject(err);
          else resolve(response.content);
        }
      );
    });
  }

  validateSession(platform: Platform): Promise<boolean> {
    return new Promise((resolve, reject) => {
      this.sessionClient.ValidateSession(
        { platform },
        this.buildMetadata(),
        (err: any, response: any) => {
          if (err) reject(err);
          else resolve(Boolean(response.valid));
        }
      );
    });
  }

  getConnectionStatus(): Promise<Record<string, boolean>> {
    return new Promise((resolve, reject) => {
      this.sessionClient.GetConnectionStatus(
        {},
        this.buildMetadata(),
        (err: any, response: any) => {
          if (err) reject(err);
          else resolve(response.platforms || {});
        }
      );
    });
  }

  // --- credentials --------------------------------------------------------

  saveCredentials(
    platform: Platform,
    username: string,
    password: string,
    twofaMethod?: string
  ): Promise<any> {
    return new Promise((resolve, reject) => {
      this.credentialClient.Save(
        {
          platform,
          username,
          password,
          twofa_method: twofaMethod || "",
        },
        this.buildMetadata(),
        (err: any, response: any) => {
          if (err) reject(err);
          else resolve(response);
        }
      );
    });
  }

  getCredentials(platform: Platform): Promise<CredentialInfo | null> {
    return new Promise((resolve, reject) => {
      this.credentialClient.Get(
        { platform },
        this.buildMetadata(),
        (err: any, response: any) => {
          if (err) {
            if (err.code === grpc.status.NOT_FOUND) resolve(null);
            else reject(err);
          } else {
            resolve({
              platform: response.platform,
              username: response.username,
              hasPassword: response.has_password,
              twofaMethod: response.twofa_method || "",
            });
          }
        }
      );
    });
  }

  deleteCredentials(platform: Platform): Promise<boolean> {
    return new Promise((resolve, reject) => {
      this.credentialClient.Delete(
        { platform },
        this.buildMetadata(),
        (err: any) => {
          if (err) {
            if (err.code === grpc.status.NOT_FOUND) resolve(false);
            else reject(err);
          } else resolve(true);
        }
      );
    });
  }

  close(): void {
    grpc.closeClient(this.authClient);
    grpc.closeClient(this.sessionClient);
    grpc.closeClient(this.credentialClient);
  }
}
