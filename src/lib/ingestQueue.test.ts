import { expect, test, vi } from "vitest";
import type { Candidate, IngestOutcome, IngestResult } from "../api";
import { type Decision, type Row, runQueue } from "./ingestQueue";

type Policy = "ask" | "replace" | "skip";

const cand = (name: string): Candidate => ({ path: `/logs/was01/${name}`, server: "was01", filename: name, size: 0 });

const res = (outcome: IngestOutcome, extra: Partial<IngestResult> = {}): IngestResult => ({ outcome, deleted: false, ...extra });

const ingested = (): IngestOutcome => ({
  kind: "ingested",
  fileId: 1,
  lines: 100,
  failedLines: 0,
  foreignRequests: 7,
  dateFrom: "2026-11-22",
  dateTo: "2026-11-22",
});

const highFailure = (): IngestOutcome => ({ kind: "highFailure", lines: 100, failedLines: 30, samples: ["bad line"] });

/** answer(filename, conflict, allowHighFailure)로 결과를 정하는 가짜 deps. 호출 기록과 행 갱신을 모은다. */
function setup(
  answer: (name: string, conflict: Policy, allow: boolean) => IngestResult | Error,
  conflictAnswer: { decision: Decision; applyToAll: boolean } = { decision: "replace", applyToAll: false },
  highFailureAnswer: (name: string) => boolean = () => true,
) {
  const calls: [string, Policy, boolean][] = [];
  const updates: [number, Row][] = [];
  const askConflict = vi.fn(async (_c: Candidate) => conflictAnswer);
  const askHighFailure = vi.fn(async (c: Candidate, _l: number, _f: number, _s: string[]) => highFailureAnswer(c.filename));
  const deps = {
    ingestOne: async (c: Candidate, conflict: Policy, allow: boolean) => {
      calls.push([c.filename, conflict, allow]);
      const r = answer(c.filename, conflict, allow);
      if (r instanceof Error) throw r;
      return r;
    },
    askConflict,
    askHighFailure,
    onRow: (i: number, row: Row) => updates.push([i, row]),
  };
  return { calls, updates, askConflict, askHighFailure, deps };
}

test("conflict asks once, then retries with the answer", async () => {
  const { calls, askConflict, deps } = setup((_n, conflict) =>
    conflict === "ask" ? res({ kind: "conflict" }) : res(ingested()),
  );
  const rows = await runQueue([cand("a")], deps);
  expect(askConflict).toHaveBeenCalledTimes(1);
  expect(calls).toEqual([["a", "ask", false], ["a", "replace", false]]);
  expect(rows[0].state).toBe("완료");
});

test("apply-to-all skips further prompts", async () => {
  const { calls, askConflict, deps } = setup(
    (_n, conflict) => (conflict === "skip" ? res({ kind: "skipped" }) : res({ kind: "conflict" })),
    { decision: "skip", applyToAll: true },
  );
  const rows = await runQueue([cand("a"), cand("b")], deps);
  expect(askConflict).toHaveBeenCalledTimes(1);
  expect(rows.map((r) => r.state)).toEqual(["건너뜀", "건너뜀"]);
  // 둘째 파일은 처음부터 "skip"으로 호출된다
  expect(calls).toEqual([["a", "ask", false], ["a", "skip", false], ["b", "skip", false]]);
});

test("high failure retries only when confirmed", async () => {
  const { calls, askHighFailure, deps } = setup(
    (_n, _c, allow) => (allow ? res(ingested()) : res(highFailure())),
    undefined,
    (name) => name === "a",
  );
  const rows = await runQueue([cand("a"), cand("b")], deps);
  expect(askHighFailure).toHaveBeenCalledTimes(2);
  expect(askHighFailure.mock.calls[0].slice(1)).toEqual([100, 30, ["bad line"]]);
  expect(calls).toEqual([["a", "ask", false], ["a", "ask", true], ["b", "ask", false]]);
  expect(rows.map((r) => r.state)).toEqual(["완료", "건너뜀"]);
});

test("high failure after replace keeps the replace decision", async () => {
  const { calls, askConflict, deps } = setup((_n, conflict, allow) => {
    if (conflict === "ask") return res({ kind: "conflict" });
    return allow ? res(ingested()) : res(highFailure());
  });
  const rows = await runQueue([cand("a")], deps);
  expect(calls).toEqual([["a", "ask", false], ["a", "replace", false], ["a", "replace", true]]);
  expect(askConflict).toHaveBeenCalledTimes(1);
  expect(rows[0].state).toBe("완료");
});

test("a thrown error marks the row failed and continues", async () => {
  const { deps } = setup((name) => (name === "a" ? new Error("읽기 오류") : res(ingested())));
  const rows = await runQueue([cand("a"), cand("b")], deps);
  expect(rows.map((r) => r.state)).toEqual(["실패", "완료"]);
  expect(rows[0].message).toBe("읽기 오류");
});

test("duplicate is reported without prompting", async () => {
  const { askConflict, askHighFailure, deps } = setup(() => res({ kind: "duplicate" }));
  const rows = await runQueue([cand("a")], deps);
  expect(rows[0].state).toBe("중복(건너뜀)");
  expect(askConflict).not.toHaveBeenCalled();
  expect(askHighFailure).not.toHaveBeenCalled();
});

test("delete failure is carried on the row", async () => {
  const { updates, deps } = setup(() => res(ingested(), { deleted: false, deleteError: "사용 중" }));
  const rows = await runQueue([cand("a")], deps);
  expect(rows[0]).toMatchObject({ state: "완료", deleted: false, message: "적재는 완료했지만 원본을 지우지 못했습니다: 사용 중" });
  expect(rows[0].summary?.foreignRequests).toBe(7);
  // 화면은 "적재 중"을 먼저 받고 마지막에 결과를 받는다
  expect(updates.map(([i, r]) => [i, r.state])).toEqual([[0, "적재 중"], [0, "완료"]]);
});
