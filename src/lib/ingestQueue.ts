// 적재 순서 로직. 화면과 분리해 테스트한다.
// 파일을 하나씩 순서대로 넣고, 충돌·실패 줄 과다는 물어본 뒤 같은 파일을 다시 호출한다.
import type { Candidate, ConflictPolicy, FileSummary, IngestResult } from "../api";

export type Decision = "replace" | "skip";
export type RowState = "대기" | "적재 중" | "완료" | "중복(건너뜀)" | "건너뜀" | "실패";
export type Row = { candidate: Candidate; state: RowState; summary?: FileSummary; message?: string; deleted?: boolean };

export type QueueDeps = {
  ingestOne(c: Candidate, conflict: ConflictPolicy, allowHighFailure: boolean): Promise<IngestResult>;
  askConflict(c: Candidate): Promise<{ decision: Decision; applyToAll: boolean }>;
  askHighFailure(c: Candidate, lines: number, failed: number, samples: string[]): Promise<boolean>;
  onRow(index: number, row: Row): void;
};

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** 한 파일을 끝낼 때까지(물어보고 다시 호출하는 것 포함) 처리한다.
 *  "남은 파일에도 똑같이 적용"을 고르면 shared.decided에 남긴다(이 파일이 나중에 실패해도 유지). */
async function runOne(c: Candidate, deps: QueueDeps, shared: { decided: Decision | null }): Promise<Row> {
  let conflict: ConflictPolicy = shared.decided ?? "ask";
  let allowHighFailure = false;
  for (;;) {
    const { outcome, deleted, deleteError } = await deps.ingestOne(c, conflict, allowHighFailure);
    switch (outcome.kind) {
      case "ingested": {
        const { kind: _, ...summary } = outcome;
        const row: Row = { candidate: c, state: "완료", summary, deleted };
        if (deleteError) row.message = `적재는 완료했지만 원본을 지우지 못했습니다: ${deleteError}`;
        return row;
      }
      case "duplicate":
        return { candidate: c, state: "중복(건너뜀)" };
      case "skipped":
        return { candidate: c, state: "건너뜀" };
      case "conflict": {
        // 이미 대체/건너뛰기를 정해 보냈는데 또 충돌이면 되묻지 않는다(무한 반복 방지).
        if (conflict !== "ask") throw new Error("적재 결과를 처리하지 못했습니다(충돌이 반복됨).");
        const answer = await deps.askConflict(c);
        if (answer.applyToAll) shared.decided = answer.decision;
        conflict = answer.decision;
        break;
      }
      case "highFailure": {
        if (allowHighFailure) throw new Error("적재 결과를 처리하지 못했습니다(실패 줄 경고가 반복됨).");
        const go = await deps.askHighFailure(c, outcome.lines, outcome.failedLines, outcome.samples);
        if (!go) return { candidate: c, state: "건너뜀" };
        // conflict는 이미 정한 값을 그대로 둔다.
        allowHighFailure = true;
        break;
      }
    }
  }
}

export async function runQueue(candidates: Candidate[], deps: QueueDeps): Promise<Row[]> {
  const rows: Row[] = candidates.map((candidate) => ({ candidate, state: "대기" }));
  const shared = { decided: null as Decision | null };
  for (let i = 0; i < candidates.length; i++) {
    const c = candidates[i];
    rows[i] = { candidate: c, state: "적재 중" };
    deps.onRow(i, rows[i]);
    try {
      rows[i] = await runOne(c, deps, shared);
    } catch (e) {
      rows[i] = { candidate: c, state: "실패", message: errorText(e) };
    }
    deps.onRow(i, rows[i]);
  }
  return rows;
}
