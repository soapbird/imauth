import type * as grpc from "@grpc/grpc-js";
import type * as protoLoader from "@grpc/proto-loader";
import type {
  CredentialGrpcClient,
  GrpcUnaryCallback,
  PlatformRequest,
  SaveCredentialsRequest,
} from "./grpc_contracts";
import { parseCredentialInfo, parseCredentialSaveResult, responseDeserializer } from "./grpc_wire";
import type { CredentialInfo, CredentialSaveResult } from "./types";

interface CredentialMethods {
  readonly save: protoLoader.MethodDefinition<object, object>;
  readonly get: protoLoader.MethodDefinition<object, object>;
  readonly delete: protoLoader.MethodDefinition<object, object>;
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
      this.methods.save.requestSerialize,
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
      this.methods.get.requestSerialize,
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
      this.methods.delete.requestSerialize,
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
