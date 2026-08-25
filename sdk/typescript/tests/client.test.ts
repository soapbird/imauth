import { ImauthClient, Platform, AuthStatus } from "../src";

describe("ImauthClient", () => {
  let client: ImauthClient;

  beforeEach(() => {
    client = new ImauthClient("localhost:6100");
  });

  afterEach(() => {
    client.close();
  });

  it("should create a client with default address", () => {
    const defaultClient = new ImauthClient();
    expect(defaultClient).toBeInstanceOf(ImauthClient);
    defaultClient.close();
  });

  it("should create a client with custom address", () => {
    expect(client).toBeInstanceOf(ImauthClient);
  });

  it("should expose Platform enum values", () => {
    expect(Platform.UNSPECIFIED).toBe(0);
    expect(Platform.INSTAGRAM).toBe(1);
    expect(Platform.THREADS).toBe(2);
    expect(Platform.NAVER).toBe(3);
  });

  it("should expose AuthStatus enum values", () => {
    expect(AuthStatus.UNSPECIFIED).toBe(0);
    expect(AuthStatus.IDLE).toBe(1);
    expect(AuthStatus.WAITING_FOR_USER).toBe(4);
    expect(AuthStatus.CONNECTED).toBe(7);
    expect(AuthStatus.FAILED).toBe(8);
  });
});
