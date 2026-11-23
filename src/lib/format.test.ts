import { expect, test } from "vitest";
import { dateTime, delta, num, pct } from "./format";

test("number formatting never shows NaN", () => {
  expect(num(4812330)).toBe("4,812,330");
  expect(pct(0.0204)).toBe("2.0%");
  expect(pct(0 / 0)).toBe("0.0%");
  expect(pct(1 / 0)).toBe("0.0%");
  expect(delta(103.1, 100)).toBe("+3.1%");
  expect(delta(98, 100)).toBe("-2.0%");
  expect(delta(5, 0)).toBe("–");
  expect(dateTime(1795359480)).toBe("11/22 23:58"); // 2026-11-22T23:58:00+09:00
});
