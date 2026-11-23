import { expect, test } from "vitest";
import { periodLabel, reportFootnotes, reportTitle, summarySentence } from "./summary";

test("summary sentence with three top countries", () => {
  expect(summarySentence({ days: 7, countries: 41, foreignIps: 3154, foreignShare: 0.02,
    top: [{ name: "미국", share: 0.33 }, { name: "일본", share: 0.2 }, { name: "중국", share: 0.13 }] }))
    .toBe("지난 7일간 41개국 3,154개 IP가 해외에서 접속했습니다. 전체 요청의 2.0%이며, 상위 3개국(미국·일본·중국)이 해외 요청의 66.0%를 차지합니다.");
});
test("summary sentence with one country", () => {
  expect(summarySentence({ days: 1, countries: 1, foreignIps: 2, foreignShare: 0.5, top: [{ name: "미국", share: 1 }] }))
    .toBe("지난 1일간 1개국 2개 IP가 해외에서 접속했습니다. 전체 요청의 50.0%이며, 상위 1개국(미국)이 해외 요청의 100.0%를 차지합니다.");
});
test("summary sentence with no foreign access", () => {
  expect(summarySentence({ days: 7, countries: 0, foreignIps: 0, foreignShare: 0, top: [] })).toBe("지난 7일간 해외 접속이 없었습니다.");
});
test("title and footnotes", () => {
  expect(reportTitle("")).toBe("해외 접속 현황");
  expect(reportTitle("민원24")).toBe("민원24 해외 접속 현황");
  expect(reportFootnotes({ exclude404: false, excludedIps: 318, geoBuildDate: "2026-11-01" })).toHaveLength(2);
  expect(reportFootnotes({ exclude404: true, excludedIps: 318, geoBuildDate: "2026-11-01" })[1])
    .toBe("404 응답을 받은 IP 318개(공격 시도 추정)는 집계에서 제외했습니다.");
});

// 이하 추가 테스트(브리프 밖): 4개를 넘는 상위국은 3개까지만, 기간 표기, 하단 안내 전체 문구
test("summary sentence uses at most three top countries", () => {
  expect(summarySentence({ days: 30, countries: 5, foreignIps: 1234567, foreignShare: 0.1,
    top: [{ name: "미국", share: 0.4 }, { name: "일본", share: 0.3 }, { name: "중국", share: 0.1 }, { name: "독일", share: 0.1 }] }))
    .toBe("지난 30일간 5개국 1,234,567개 IP가 해외에서 접속했습니다. 전체 요청의 10.0%이며, 상위 3개국(미국·일본·중국)이 해외 요청의 80.0%를 차지합니다.");
});
test("period label", () => {
  expect(periodLabel({ from: "2026-11-16", to: "2026-11-22" })).toBe("2026. 11. 16. ~ 11. 22. (7일)");
  expect(periodLabel({ from: "2025-12-29", to: "2026-01-04" })).toBe("2025. 12. 29. ~ 2026. 1. 4. (7일)");
  expect(periodLabel({ from: "2026-11-22", to: "2026-11-22" })).toBe("2026. 11. 22. (1일)");
  expect(periodLabel({ from: "bad", to: "2026-11-22" })).toBe("bad ~ 2026-11-22");
});
test("footnotes in order", () => {
  expect(reportFootnotes({ exclude404: true, excludedIps: 1234, geoBuildDate: "2026-11-01" })).toEqual([
    "국가는 접속 IP 주소 기준으로 판별했으며, VPN 등으로 실제 위치와 다를 수 있습니다.",
    "404 응답을 받은 IP 1,234개(공격 시도 추정)는 집계에서 제외했습니다.",
    "IP 위치 정보: DB-IP (db-ip.com), CC BY 4.0, 2026-11-01 기준",
  ]);
  expect(reportTitle("  민원24 ")).toBe("민원24 해외 접속 현황");
});
