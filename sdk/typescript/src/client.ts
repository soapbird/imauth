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
  }
);

const proto = grpc.loadPackageDefinition(packageDefinition) as any;

export class ImauthClient {
  private authClient: any;
  private sessionClient: any;
  private credentialClient: any;

  constructor(serverAddress: string = "localhost:50051") {
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

  login(
    platform: Platform,
    username: string,
    password: string
  ): grpc.ClientReadableStream<any> {
    return this.authClient.Login({ platform, username, password });
  }

  submit2FA(sessionId: string, code: string): Promise<any> {
    return new Promise((resolve, reject) => {
      this.authClient.Submit2FA({ sessionId, code }, (err: any, response: any) => {
        if (err) reject(err);
        else resolve(response);
      });
    });
  }

  getCookies(platform: Platform): Promise<Cookie[]> {
    return new Promise((resolve, reject) => {
      this.sessionClient.GetCookies({ platform }, (err: any, response: any) => {
        if (err) reject(err);
        else resolve(response.cookies || []);
      });
    });
  }

  exportNetscape(platform: Platform): Promise<string> {
    return new Promise((resolve, reject) => {
      this.sessionClient.ExportNetscape({ platform }, (err: any, response: any) => {
        if (err) reject(err);
        else resolve(response.content);
      });
    });
  }

  getConnectionStatus(): Promise<Record<string, boolean>> {
    return new Promise((resolve, reject) => {
      this.sessionClient.GetConnectionStatus({}, (err: any, response: any) => {
        if (err) reject(err);
        else resolve(response.platforms || {});
      });
    });
  }

  saveCredentials(
    platform: Platform,
    username: string,
    password: string,
    twofaMethod?: string
  ): Promise<any> {
    return new Promise((resolve, reject) => {
      this.credentialClient.Save(
        { platform, username, password, twofaMethod: twofaMethod || "" },
        (err: any, response: any) => {
          if (err) reject(err);
          else resolve(response);
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
