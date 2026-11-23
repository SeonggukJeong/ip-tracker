// 로그 적재: 파일·폴더 넣기 → 목록(서버 이름 고치기) → 미리보기로 형식 확인 → 적재 시작 → 파일별 결과와 요약.
import { useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  type Candidate,
  getSettings,
  type IngestProgress,
  ingestOne,
  onIngestProgress,
  type Preview,
  previewFile,
  saveSettings,
  scanPaths,
} from "../api";
import { type FormatInput, IngestPreview } from "../components/IngestPreview";
import { IngestDialog, type Prompt } from "../components/IngestDialogs";
import { bytes, num, pct, ratio } from "../lib/format";
import { type Decision, type Row, runQueue } from "../lib/ingestQueue";

const DELETE_KEY = "iptracker.deleteOriginal";

function loadDeleteChoice(): boolean {
  try {
    return localStorage.getItem(DELETE_KEY) === "1";
  } catch {
    return false;
  }
}

function storeDeleteChoice(on: boolean) {
  try {
    localStorage.setItem(DELETE_KEY, on ? "1" : "0");
  } catch {
    // 저장하지 못해도 이번 선택은 그대로 쓴다.
  }
}

/** 미리보기에 실제로 쓴 형식. 비운 칸은 Rust 쪽에서 저장값 → 자동 인식 순으로 정한다. */
type Applied = { logFormat: string; timeFormat: string };

type PreviewState = { path: string; preview: Preview; timeFormat: string };

const FINISHED: Row["state"][] = ["완료", "중복(건너뜀)", "건너뜀", "실패"];

type Props = {
  /** 탭이 보일 때만 끌어다 놓기를 받는다 */
  active: boolean;
  /** 적재가 끝나면 호출(대시보드 새로고침) */
  onIngested: () => void;
};

export function Ingest({ active, onIngested }: Props) {
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [selected, setSelected] = useState(0);
  const [scanError, setScanError] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);

  const [applied, setApplied] = useState<Applied>({ logFormat: "", timeFormat: "" });
  const [pv, setPv] = useState<PreviewState | null>(null);
  const [pvLoading, setPvLoading] = useState(false);
  const [pvError, setPvError] = useState<string | null>(null);
  const [formatOpen, setFormatOpen] = useState(false);
  const [input, setInput] = useState<FormatInput>({ logFormat: "", timeFormat: "", save: false });
  const [saveNote, setSaveNote] = useState<string | null>(null);
  const pendingSave = useRef(false);

  const [deleteOriginal, setDeleteOriginal] = useState(loadDeleteChoice);
  const [running, setRunning] = useState(false);
  const [rows, setRows] = useState<Row[] | null>(null);
  const [progress, setProgress] = useState<IngestProgress | null>(null);
  const [prompt, setPrompt] = useState<Prompt | null>(null);

  // ---- 파일 넣기 ----

  const addPaths = async (paths: string[]) => {
    if (running || paths.length === 0) return;
    setScanError(null);
    try {
      const found = await scanPaths(paths);
      if (found.length === 0) {
        setScanError("넣은 곳에서 파일을 찾지 못했습니다.");
        return;
      }
      // 적재를 마친 목록에 새로 넣으면 새 목록으로 시작한다.
      const fresh = rows !== null;
      const base = fresh ? [] : candidates;
      const known = new Set(base.map((c) => c.path));
      setCandidates([...base, ...found.filter((c) => !known.has(c.path))]);
      if (fresh) {
        setRows(null);
        setSelected(0);
      }
    } catch (e) {
      setScanError(String(e));
    }
  };

  // 끌어다 놓기 핸들러는 한 번만 등록하므로 최신 상태는 ref로 본다.
  const addPathsRef = useRef(addPaths);
  addPathsRef.current = addPaths;
  const activeRef = useRef(active);
  activeRef.current = active;

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let dead = false;
    try {
      getCurrentWebview()
        .onDragDropEvent((e) => {
          if (!activeRef.current) return;
          const p = e.payload;
          if (p.type === "enter" || p.type === "over") setDragging(true);
          else if (p.type === "leave") setDragging(false);
          else if (p.type === "drop") {
            setDragging(false);
            void addPathsRef.current(p.paths);
          }
        })
        .then((f) => (dead ? f() : (unlisten = f)));
    } catch {
      // Tauri 밖(브라우저 미리보기)에서는 끌어다 놓기를 쓰지 않는다.
    }
    return () => {
      dead = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let dead = false;
    onIngestProgress(setProgress).then(
      (f) => (dead ? f() : (unlisten = f)),
      () => undefined,
    );
    return () => {
      dead = true;
      unlisten?.();
    };
  }, []);

  const pick = async (directory: boolean) => {
    try {
      const chosen = await open({ multiple: true, directory });
      if (chosen) await addPaths(chosen);
    } catch (e) {
      setScanError(String(e));
    }
  };

  const clearList = () => {
    setCandidates([]);
    setRows(null);
    setSelected(0);
    setScanError(null);
  };

  const removeAt = (i: number) => {
    setCandidates((cs) => cs.filter((_, j) => j !== i));
    setSelected((s) => (i < s ? s - 1 : i === s ? 0 : s));
  };

  const setServer = (i: number, server: string) =>
    setCandidates((cs) => cs.map((c, j) => (j === i ? { ...c, server } : c)));

  // ---- 미리보기 ----

  const previewPath = candidates[selected]?.path ?? null;

  useEffect(() => {
    if (previewPath === null) {
      setPv(null);
      setPvError(null);
      setPvLoading(false);
      return;
    }
    let live = true;
    setPvLoading(true);
    setPvError(null);
    const used = applied;
    previewFile(previewPath, used.logFormat.trim() || undefined, used.timeFormat.trim() || undefined).then(
      (preview) => {
        if (!live) return;
        setPv({ path: previewPath, preview, timeFormat: used.timeFormat.trim() });
        setPvLoading(false);
        // 인식하지 못했으면 입력 칸을 열어 둔다(다음 미리보기가 오류로 끝나도 칸이 사라지지 않게).
        if (preview.logFormat === null) setFormatOpen(true);
        if (pendingSave.current) {
          pendingSave.current = false;
          void saveFormat(preview, used.timeFormat.trim());
        }
      },
      (e) => {
        if (!live) return;
        setPv(null);
        setPvError(String(e));
        setPvLoading(false);
        pendingSave.current = false;
      },
    );
    return () => {
      live = false;
    };
  }, [previewPath, applied]);

  const saveFormat = async (preview: Preview, timeFormat: string) => {
    if (preview.logFormat === null || preview.rows.length === 0) {
      setSaveNote("이 형식으로 읽은 줄이 없어 저장하지 않았습니다.");
      return;
    }
    try {
      const s = await getSettings();
      await saveSettings({ ...s, logFormat: preview.logFormat, timeFormat: timeFormat || s.timeFormat });
      setSaveNote("형식을 저장했습니다.");
    } catch (e) {
      setSaveNote(`형식을 저장하지 못했습니다: ${String(e)}`);
    }
  };

  const openFormat = async () => {
    setFormatOpen(true);
    let timeFormat = input.timeFormat;
    if (!timeFormat) {
      try {
        timeFormat = (await getSettings()).timeFormat;
      } catch {
        // 저장값을 못 읽으면 빈 칸으로 둔다.
      }
    }
    setInput((v) => ({ ...v, logFormat: v.logFormat || pv?.preview.logFormat || "", timeFormat: v.timeFormat || timeFormat }));
  };

  const repreview = () => {
    setFormatOpen(true);
    pendingSave.current = input.save;
    setSaveNote(null);
    setApplied({ logFormat: input.logFormat, timeFormat: input.timeFormat });
  };

  // 적재에 넘길 형식: 지금 보이는 미리보기에서 확정된 것. 한 줄도 읽지 못한 형식은 확정으로 보지 않는다.
  const current = pv !== null && pv.path === previewPath && !pvLoading ? pv : null;
  const confirmed =
    current?.preview.logFormat && current.preview.rows.length > 0
      ? { logFormat: current.preview.logFormat, timeFormat: current.timeFormat || undefined }
      : null;

  // ---- 적재 ----

  const blankServer = candidates.some((c) => c.server.trim() === "");
  const canStart = !running && rows === null && candidates.length > 0 && confirmed !== null && !blankServer;

  const askConflict = (candidate: Candidate) =>
    new Promise<{ decision: Decision; applyToAll: boolean }>((resolve) =>
      setPrompt({
        kind: "conflict",
        candidate,
        resolve: (a) => {
          setPrompt(null);
          resolve(a);
        },
      }),
    );

  const askHighFailure = (candidate: Candidate, lines: number, failed: number, samples: string[]) =>
    new Promise<boolean>((resolve) =>
      setPrompt({
        kind: "highFailure",
        candidate,
        lines,
        failed,
        samples,
        resolve: (go) => {
          setPrompt(null);
          resolve(go);
        },
      }),
    );

  const start = async () => {
    if (!canStart || confirmed === null) return;
    const fmt = confirmed;
    const del = deleteOriginal;
    const list = candidates.map((c) => ({ ...c, server: c.server.trim() }));
    setCandidates(list);
    setRows(list.map((candidate) => ({ candidate, state: "대기" })));
    setProgress(null);
    setRunning(true);
    try {
      await runQueue(list, {
        ingestOne: (candidate, conflict, allowHighFailure) =>
          ingestOne({
            candidate,
            conflict,
            allowHighFailure,
            deleteOriginal: del,
            logFormat: fmt.logFormat,
            timeFormat: fmt.timeFormat,
          }),
        askConflict,
        askHighFailure,
        onRow: (i, row) => setRows((rs) => rs && rs.map((r, j) => (j === i ? row : r))),
      });
    } finally {
      setRunning(false);
      setProgress(null);
      onIngested();
    }
  };

  const toggleDelete = (on: boolean) => {
    setDeleteOriginal(on);
    storeDeleteChoice(on);
  };

  // ---- 표시 ----

  const doneCount = rows?.filter((r) => FINISHED.includes(r.state)).length ?? 0;
  const finished = rows !== null && !running;
  const count = (s: Row["state"]) => rows?.filter((r) => r.state === s).length ?? 0;
  const foreign = rows?.reduce((sum, r) => sum + (r.state === "완료" ? (r.summary?.foreignRequests ?? 0) : 0), 0) ?? 0;

  return (
    <div className="ingest">
      <div className={`dropzone${dragging ? " over" : ""}`}>
        <p>로그 파일이나 폴더를 여기에 끌어다 놓으세요 (.gz 포함)</p>
        <div className="bar">
          <button type="button" className="btn" onClick={() => pick(false)} disabled={running}>
            파일 선택
          </button>
          <button type="button" className="btn" onClick={() => pick(true)} disabled={running}>
            폴더 선택
          </button>
        </div>
      </div>
      {scanError && <p className="error">{scanError}</p>}

      {candidates.length > 0 && (
        <section className="panel">
          <div className="panel-head">
            <h4>적재할 파일 {num(candidates.length)}개</h4>
            {rows !== null && (
              <span className="muted">
                {num(doneCount)}/{num(rows.length)}
              </span>
            )}
            <span className="sp" />
            <button type="button" className="btn" onClick={clearList} disabled={running}>
              목록 비우기
            </button>
          </div>
          <div className="file-list">
            <table className="grid files clickable">
              <thead>
                <tr>
                  <th>서버 이름</th>
                  <th>파일명</th>
                  <th>크기</th>
                  <th>상태</th>
                  <th>결과</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {candidates.map((c, i) => {
                  const row = rows?.[i];
                  return (
                    <tr
                      key={c.path}
                      className={i === selected ? "sel" : undefined}
                      onClick={() => !running && setSelected(i)}
                      title={c.path}
                    >
                      <td>
                        <input
                          className={`server${c.server.trim() === "" ? " bad" : ""}`}
                          value={c.server}
                          onChange={(e) => setServer(i, e.target.value)}
                          onClick={(e) => e.stopPropagation()}
                          disabled={running || rows !== null}
                          aria-label="서버 이름"
                        />
                      </td>
                      <td className="wrap">{c.filename}</td>
                      <td className="nowrap">{bytes(c.size)}</td>
                      <td className="nowrap">
                        <StateLabel state={row?.state ?? "대기"} />
                      </td>
                      <td>
                        <RowResult row={row} progress={progress} />
                      </td>
                      <td>
                        {rows === null && (
                          <button
                            type="button"
                            className="link"
                            onClick={(e) => {
                              e.stopPropagation();
                              removeAt(i);
                            }}
                            aria-label="목록에서 빼기"
                            title="목록에서 빼기"
                          >
                            ×
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {blankServer && <p className="error">서버 이름이 빈 파일이 있습니다. 서버 이름을 채워 주세요.</p>}
        </section>
      )}

      {candidates.length > 0 && (
        <IngestPreview
          filename={candidates[selected]?.filename ?? null}
          preview={current?.preview ?? null}
          loading={pvLoading}
          error={pvError}
          formatOpen={formatOpen}
          onOpenFormat={openFormat}
          input={input}
          onInput={setInput}
          onRepreview={repreview}
          saveNote={saveNote}
          disabled={running}
        />
      )}

      {candidates.length > 0 && (
        <div className="start-bar">
          <label className="check">
            <input type="checkbox" checked={deleteOriginal} onChange={(e) => toggleDelete(e.target.checked)} disabled={running} />
            적재가 끝나면 원본 파일 삭제
          </label>
          <span className="muted">DB에 저장이 끝난 파일만 지웁니다. 휴지통을 거치지 않습니다.</span>
          <span className="sp" />
          <button type="button" className="btn primary" onClick={start} disabled={!canStart}>
            {running ? "적재 중…" : "적재 시작"}
          </button>
        </div>
      )}

      {finished && rows !== null && (
        <p className="summary">
          적재 {num(count("완료"))}개 · 중복 {num(count("중복(건너뜀)"))}개 · 건너뜀 {num(count("건너뜀"))}개 · 실패{" "}
          {num(count("실패"))}개, 해외 요청 {num(foreign)}건
        </p>
      )}

      {prompt && <IngestDialog key={`${prompt.kind}:${prompt.candidate.path}`} prompt={prompt} />}
    </div>
  );
}

function StateLabel({ state }: { state: Row["state"] }) {
  const cls = state === "완료" ? "ok" : state === "실패" ? "error" : state === "적재 중" ? "busy" : "muted";
  return <span className={cls}>{state}</span>;
}

function RowResult({ row, progress }: { row: Row | undefined; progress: IngestProgress | null }) {
  if (!row) return null;
  if (row.state === "적재 중") {
    const r = progress && progress.path === row.candidate.path ? ratio(progress.bytesRead, progress.bytesTotal) : 0;
    return (
      <span className="prog-wrap">
        <span className="prog">
          <i style={{ width: `${Math.min(100, r * 100)}%` }} />
        </span>
        <span className="muted">{pct(r, 0)}</span>
      </span>
    );
  }
  if (row.state === "실패") return <span className="error">{row.message}</span>;
  if (row.state !== "완료" || !row.summary) return null;
  const s = row.summary;
  return (
    <>
      <span>
        {num(s.lines)}줄 · 실패 {num(s.failedLines)}줄 · 해외 {num(s.foreignRequests)}건{row.deleted ? " · 원본 삭제함" : ""}
      </span>
      {row.message && <div className="warn">{row.message}</div>}
    </>
  );
}
