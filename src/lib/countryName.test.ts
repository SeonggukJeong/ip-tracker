import { expect, test } from "vitest";
import { countryName } from "./countryName";

test("country names are Korean", () => {
  expect(countryName("US")).toBe("미국");
  expect(countryName("JP")).toBe("일본");
  expect(countryName("XX")).toBe("XX");
});
