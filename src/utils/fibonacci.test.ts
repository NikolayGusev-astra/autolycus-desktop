import { describe, expect, it } from "vitest";

import { fibonacci } from "./fibonacci";

describe("fibonacci", () => {
  it("returns values from the Fibonacci sequence", () => {
    expect(fibonacci(0)).toBe(0);
    expect(fibonacci(1)).toBe(1);
    expect(fibonacci(10)).toBe(55);
    expect(fibonacci(78)).toBe(8944394323791464);
  });

  it("rejects indexes outside the supported range", () => {
    expect(() => fibonacci(-1)).toThrow(RangeError);
    expect(() => fibonacci(1.5)).toThrow(RangeError);
    expect(() => fibonacci(79)).toThrow(RangeError);
  });
});
