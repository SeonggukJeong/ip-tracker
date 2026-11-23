import { expect, test } from "vitest";
import { bytes, dateTime, delta, num, pct } from "./format";

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

test("file sizes", () => {
  expect(bytes(812)).toBe("812 B");
  expect(bytes(1536)).toBe("1.5 KB");
  expect(bytes(52428800)).toBe("50.0 MB");
  expect(bytes(5 * 1024 ** 4 * 1000)).toBe("5,000.0 TB");
  expect(bytes(-1)).toBe("0 B");
});
