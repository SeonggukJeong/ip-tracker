import { expect, test } from "vitest";
import { checkCustomPeriod, monthsBefore, periodDays, presetPeriod } from "./period";

test("custom period accepts a normal range up to 366 days", () => {
  expect(checkCustomPeriod("2026-11-01", "2026-11-23")).toEqual({ ok: true, period: { from: "2026-11-01", to: "2026-11-23" } });
  expect(checkCustomPeriod("2026-11-23", "2026-11-23").ok).toBe(true);
  // 윤년이 낀 366일
  expect(checkCustomPeriod("2027-03-01", "2028-02-29").ok).toBe(true);
  expect(checkCustomPeriod("2000-01-01", "2000-01-01").ok).toBe(true);
});

test("custom period refuses more than 366 days", () => {
  expect(checkCustomPeriod("2027-03-01", "2028-03-01")).toEqual({ ok: false, error: "기간은 최대 366일까지 지정할 수 있습니다." });
  expect(checkCustomPeriod("2020-01-01", "2026-11-23")).toEqual({ ok: false, error: "기간은 최대 366일까지 지정할 수 있습니다." });
});

test("custom period refuses dates before 2000 and from after to", () => {
  // 연도를 한 자리씩 칠 때 들어오는 값
  expect(checkCustomPeriod("0002-11-01", "2026-11-23")).toEqual({ ok: false, error: "2000-01-01 이후 날짜를 지정해 주세요." });
  expect(checkCustomPeriod("2026-11-01", "0202-11-23")).toEqual({ ok: false, error: "2000-01-01 이후 날짜를 지정해 주세요." });
  expect(checkCustomPeriod("1999-12-31", "2000-01-05").ok).toBe(false);
  expect(checkCustomPeriod("2026-11-23", "2026-11-01")).toEqual({ ok: false, error: "시작일이 종료일보다 늦습니다." });
});

test("incomplete or invalid input is ignored without a message", () => {
  expect(checkCustomPeriod("", "2026-11-23")).toEqual({ ok: false, error: null });
  expect(checkCustomPeriod("2026-02-30", "2026-03-01")).toEqual({ ok: false, error: null });
  expect(checkCustomPeriod("2026-11-1", "2026-11-23")).toEqual({ ok: false, error: null });
});

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
