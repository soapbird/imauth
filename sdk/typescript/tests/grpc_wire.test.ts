import * as protoLoader from "@grpc/proto-loader";
import * as path from "node:path";
import { AuthStatus } from "../src";
import type { CancelRequest, StatusRequest } from "../src/grpc_contracts";
import { GrpcResponseError, parseAuthEvent } from "../src/grpc_wire";

const PROTO_ROOT = path.join(__dirname, "../proto");
const packageDefinition = protoLoader.loadSync(path.join(PROTO_ROOT, "imauth/v1/auth.proto"), {
  keepCase: false,
  longs: Number,
  defaults: true,
  oneofs: true,
  includeDirs: [PROTO_ROOT],
});

function authMethod(methodName: string): protoLoader.MethodDefinition<object, object> {
  const service = packageDefinition["imauth.v1.AuthService"];
  if (service === undefined || "format" in service) {
    throw new Error("Missing AuthService package definition");
  }
  const method = service[methodName];
  if (method === undefined) {
    throw new Error(`Missing AuthService method definition: ${methodName}`);
  }
  return method;
}

function roundTripRequest<Request extends object>(methodName: string, request: Request): object {
  const method = authMethod(methodName);
  return method.requestDeserialize(method.requestSerialize(request));
}

describe("gRPC response parsing", () => {
  it("maps a decoded AuthEvent to the public shape", () => {
    const event = parseAuthEvent({
      status: AuthStatus.WAITING_FOR_USER,
      sessionId: "session-1",
      message: "continue in browser",
      requiresInput: true,
      inputType: "viewer_url",
      cookies: [],
      viewerUrl: "https://viewer.example/session-1",
    });

    expect(Object.keys(event).sort()).toEqual([
      "cookies",
      "inputType",
      "message",
      "requiresInput",
      "sessionId",
      "status",
      "viewerUrl",
    ]);
  });

  it("rejects malformed response fields with a typed error", () => {
    expect(() =>
      parseAuthEvent({
        status: "waiting",
        sessionId: "session-1",
        message: "",
        requiresInput: false,
        inputType: "",
        cookies: [],
        viewerUrl: "",
      }),
    ).toThrow(GrpcResponseError);
  });
});

describe("gRPC auth request serialization", () => {
  it("preserves the GetStatus session ID through the real proto serializer", () => {
    const request: StatusRequest = { sessionId: "status-session-distinct" };

    const decoded = roundTripRequest("GetStatus", request);

    expect(decoded).toEqual({ sessionId: "status-session-distinct" });
  });

  it("preserves the Cancel session ID through the real proto serializer", () => {
    const request: CancelRequest = { sessionId: "cancel-session-distinct" };

    const decoded = roundTripRequest("Cancel", request);

    expect(decoded).toEqual({ sessionId: "cancel-session-distinct" });
  });
});
