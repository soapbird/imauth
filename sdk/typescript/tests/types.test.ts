import { AuthStatus, Platform } from "../src";

describe("Enum and module surface", () => {
  it("keeps Platform values stable", () => {
    expect(Platform.UNSPECIFIED).toBe(0);
    expect(Platform.INSTAGRAM).toBe(1);
    expect(Platform.THREADS).toBe(2);
    expect(Platform.NAVER).toBe(3);
  });

  it("keeps AuthStatus values stable", () => {
    expect(AuthStatus.IDLE).toBe(1);
    expect(AuthStatus.LOADING).toBe(2);
    expect(AuthStatus.AUTHENTICATING).toBe(3);
    expect(AuthStatus.WAITING_FOR_USER).toBe(4);
    expect(AuthStatus.CONNECTED).toBe(7);
    expect(AuthStatus.FAILED).toBe(8);
  });
});
