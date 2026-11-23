import { expect, test } from "vitest";
import { periodDays, presetPeriod } from "./period";

test("presets anchor on latest ingested date", () => {
  expect(presetPeriod("latest", "2026-11-22", "2026-11-23")).toEqual({ from: "2026-11-22", to: "2026-11-22" });
  expect(presetPeriod("7d", "2026-11-22", "2026-11-23")).toEqual({ from: "2026-11-16", to: "2026-11-22" });
  expect(presetPeriod("30d", "2026-11-22", "2026-11-23")).toEqual({ from: "2026-10-24", to: "2026-11-22" });
  expect(presetPeriod("7d", null, "2026-11-23")).toEqual({ from: "2026-11-17", to: "2026-11-23" });
  expect(periodDays({ from: "2026-11-16", to: "2026-11-22" })).toBe(7);
});
