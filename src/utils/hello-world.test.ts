import { describe, expect, it } from "vitest";

import { helloWorld } from "./hello-world";

describe("helloWorld", () => {
  it("returns the standard greeting", () => {
    expect(helloWorld()).toBe("Hello, world!");
  });
});
