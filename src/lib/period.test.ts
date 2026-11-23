import { expect, test } from "vitest";
import { monthsBefore, periodDays, presetPeriod } from "./period";

test("presets anchor on latest ingested date", () => {
  expect(presetPeriod("latest", "2026-11-22", "2026-11-23")).toEqual({ from: "2026-11-22", to: "2026-11-22" });
  expect(presetPeriod("7d", "2026-11-22", "2026-11-23")).toEqual({ from: "2026-11-16", to: "2026-11-22" });
  expect(presetPeriod("30d", "2026-11-22", "2026-11-23")).toEqual({ from: "2026-10-24", to: "2026-11-22" });
  expect(presetPeriod("7d", null, "2026-11-23")).toEqual({ from: "2026-11-17", to: "2026-11-23" });
  expect(periodDays({ from: "2026-11-16", to: "2026-11-22" })).toBe(7);
});

test("monthsBefore subtracts months and clamps the day", () => {
  expect(monthsBefore("2026-11-23", 6)).toBe("2026-05-23");
  expect(monthsBefore("2026-08-31", 6)).toBe("2026-02-28");
  expect(monthsBefore("2027-08-31", 6)).toBe("2027-02-28");
  expect(monthsBefore("2028-08-31", 6)).toBe("2028-02-29");
  expect(monthsBefore("2026-03-15", 14)).toBe("2025-01-15");
  expect(monthsBefore("2026-01-31", 12)).toBe("2025-01-31");
});
