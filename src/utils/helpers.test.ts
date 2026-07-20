import { describe, it, expect } from "vitest";

describe("helpers", () => {
  it("should pass basic test", () => {
    expect(true).toBe(true);
  });

  it("should have basic math working", () => {
    expect(1 + 1).toBe(2);
  });
});
