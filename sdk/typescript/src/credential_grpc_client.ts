import type * as grpc from "@grpc/grpc-js";
import type * as protoLoader from "@grpc/proto-loader";
import type {
  CredentialGrpcClient,
  GrpcUnaryCallback,
  PlatformRequest,
  SaveCredentialsRequest,
} from "./grpc_contracts";
import { parseCredentialInfo, parseCredentialSaveResult } from "./grpc_wire";
import type { CredentialInfo, CredentialSaveResult } from "./types";

interface CredentialMethods {
  readonly save: protoLoader.MethodDefinition<object, object>;
  readonly get: protoLoader.MethodDefinition<object, object>;
  readonly delete: protoLoader.MethodDefinition<object, object>;
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

export class CredentialClient implements CredentialGrpcClient {
  constructor(
    private readonly client: grpc.Client,
    private readonly methods: CredentialMethods,
  ) {}

  Save(
    request: SaveCredentialsRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<CredentialSaveResult>,
  ): void {
    this.client.makeUnaryRequest(
      this.methods.save.path,
      requestSerializer<SaveCredentialsRequest>(this.methods.save),
      responseDeserializer(this.methods.save, parseCredentialSaveResult),
      request,
      metadata,
      callback,
    );
  }

  Get(
    request: PlatformRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<CredentialInfo>,
  ): void {
    this.client.makeUnaryRequest(
      this.methods.get.path,
      requestSerializer<PlatformRequest>(this.methods.get),
      responseDeserializer(this.methods.get, parseCredentialInfo),
      request,
      metadata,
      callback,
    );
  }

  Delete(
    request: PlatformRequest,
    metadata: grpc.Metadata,
    callback: GrpcUnaryCallback<void>,
  ): void {
    this.client.makeUnaryRequest(
      this.methods.delete.path,
      requestSerializer<PlatformRequest>(this.methods.delete),
      responseDeserializer(this.methods.delete, () => undefined),
      request,
      metadata,
      callback,
    );
  }

  close(): void {
    this.client.close();
  }
}
