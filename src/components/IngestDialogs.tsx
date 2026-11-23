// 적재 중 묻는 창 두 가지: 같은 파일 내용이 다를 때(대체/건너뛰기), 읽지 못한 줄이 많을 때(그래도 적재/건너뛰기).
import { useState } from "react";
import type { Candidate } from "../api";
import { num } from "../lib/format";
import type { Decision } from "../lib/ingestQueue";

export type Prompt =
  | { kind: "conflict"; candidate: Candidate; resolve: (a: { decision: Decision; applyToAll: boolean }) => void }
  | { kind: "highFailure"; candidate: Candidate; lines: number; failed: number; samples: string[]; resolve: (go: boolean) => void };

export function IngestDialog({ prompt }: { prompt: Prompt }) {
  return (
    <div className="drawer-backdrop">
      <div className="modal" role="dialog" aria-modal="true">
        {prompt.kind === "conflict" ? <ConflictBody p={prompt} /> : <HighFailureBody p={prompt} />}
      </div>
    </div>
  );
}

function ConflictBody({ p }: { p: Extract<Prompt, { kind: "conflict" }> }) {
  const [applyToAll, setApplyToAll] = useState(false);
  const answer = (decision: Decision) => p.resolve({ decision, applyToAll });
  return (
    <>
      <p>
        {p.candidate.server} / {p.candidate.filename}은 이미 적재된 파일과 내용이 다릅니다. 기존 것을 대체할까요?
      </p>
      <label className="check">
        <input type="checkbox" checked={applyToAll} onChange={(e) => setApplyToAll(e.target.checked)} />
        남은 파일에도 똑같이 적용
      </label>
      <div className="modal-actions">
        <button type="button" className="btn primary" onClick={() => answer("replace")}>
          대체
        </button>
        <button type="button" className="btn" onClick={() => answer("skip")}>
          건너뛰기
        </button>
      </div>
    </>
  );
}

function HighFailureBody({ p }: { p: Extract<Prompt, { kind: "highFailure" }> }) {
  return (
    <>
      <p>
        {p.candidate.filename}에서 {num(p.lines)}줄 중 {num(p.failed)}줄을 읽지 못했습니다. 형식이 맞는지 확인해 주세요.
      </p>
      {p.samples.length > 0 && (
        <>
          <p className="muted">읽지 못한 줄 예시</p>
          <pre className="samples mono">{p.samples.join("\n")}</pre>
        </>
      )}
      <div className="modal-actions">
        <button type="button" className="btn primary" onClick={() => p.resolve(false)}>
          이 파일 건너뛰기
        </button>
        <button type="button" className="btn" onClick={() => p.resolve(true)}>
          그래도 적재
        </button>
      </div>
    </>
  );
}
