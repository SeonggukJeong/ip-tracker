// Tauri 명령의 타입과 래퍼. Rust 쪽(serde camelCase)과 이름·모양을 맞춘다.
// 명령이 실패하면 Promise가 한국어 오류 문자열로 거부된다.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ---- 적재 (iptracker_core::ingest) ----

export type Candidate = { path: string; server: string; filename: string };

/** time: "YYYY-MM-DD HH:MM:SS" KST */
export type PreviewRow = { ip: string; time: string; method: string; url: string; status: number };

/** logFormat이 null이면 형식을 정하지 못한 것. rows.length + failed === sampled */
export type Preview = { logFormat: string | null; rows: PreviewRow[]; sampled: number; failed: number };

export type ConflictPolicy = "ask" | "replace" | "skip";

export type FileSummary = {
  fileId: number;
  lines: number;
  failedLines: number;
  foreignRequests: number;
  dateFrom: string | null;
  dateTo: string | null;
};

export type IngestOutcome =
  | ({ kind: "ingested" } & FileSummary)
  | { kind: "duplicate" }
  | { kind: "conflict" }
  | { kind: "skipped" }
  | { kind: "highFailure"; lines: number; failedLines: number; samples: string[] };

export type IngestResult = { outcome: IngestOutcome; deleted: boolean; deleteError?: string };

/** 이벤트 "ingest-progress" */
export type IngestProgress = { path: string; bytesRead: number; bytesTotal: number };

// ---- 집계 (iptracker_core::query) ----

/** "YYYY-MM-DD", 양끝 포함 */
export type Period = { from: string; to: string };

export type Filter = { period: Period; exclude404: boolean };

export type PeriodStats = {
  totalRequests: number;
  foreignRequests: number;
  foreignIps: number;
  domesticIps: number;
  countries: number;
};

export type Kpis = {
  cur: PeriodStats;
  /** 바로 앞의 같은 길이 기간 */
  prev: PeriodStats;
  foreignIps404: number;
  excludedIps: number;
  newCountries: number;
  unknownRequests: number;
};

export type CountryRow = {
  country: string;
  requests: number;
  /** 해외 요청 중 비율 0~1 */
  share: number;
  ips: number;
  notFound: number;
  ips404: number;
  /** 유닉스 초 */
  lastTs: number;
};

export type DayPoint = { date: string; foreignRequests: number; foreignIps: number };

export type IpRow = { ip: string; requests: number; notFound: number; firstTs: number; lastTs: number };

export type RequestRow = {
  ts: number;
  method: string;
  url: string;
  status: number;
  bytes: number | null;
  userAgent: string | null;
  server: string;
};

export type IngestStatus = { latestDate: string | null; serversOnLatest: number; fileCount: number };

// ---- 앱 ----

/** 문자열 값은 빈 문자열 = 미지정 */
export type Settings = {
  serviceName: string;
  expectedServers: number;
  serverRegex: string;
  logFormat: string;
  timeFormat: string;
};

export type AppInfo = {
  version: string;
  geoBuildDate: string;
  geoIsBundled: boolean;
  dbPath: string;
  dbBytes: number;
  /** 설정된 DB를 열지 못해 기본 위치를 쓰는 중이면 안내 문구 */
  dbWarning: string | null;
};

/** set_db_path 뒤 쓰는 명령(적재·정리·설정 저장·DB 위치 변경)이 돌려주는 오류 */
export const DB_MOVED_ERROR = "DB 위치를 바꿨습니다. 앱을 다시 시작해 주세요.";

// ---- 명령 ----

export const scanPaths = (paths: string[]) => invoke<Candidate[]>("scan_paths", { paths });

export const previewFile = (path: string, logFormat?: string, timeFormat?: string) =>
  invoke<Preview>("preview_file", { path, logFormat, timeFormat });

export const ingestOne = (args: {
  candidate: Candidate;
  conflict: ConflictPolicy;
  allowHighFailure: boolean;
  deleteOriginal: boolean;
  logFormat?: string;
  timeFormat?: string;
}) => invoke<IngestResult>("ingest_one", args);

export const getKpis = (filter: Filter) => invoke<Kpis>("get_kpis", { filter });
export const getCountries = (filter: Filter) => invoke<CountryRow[]>("get_countries", { filter });
export const getDaily = (filter: Filter) => invoke<DayPoint[]>("get_daily", { filter });

export const getCountryIps = (filter: Filter, country: string, limit: number, offset: number) =>
  invoke<IpRow[]>("get_country_ips", { filter, country, limit, offset });

export const getIpRequests = (period: Period, ip: string, limit: number, offset: number) =>
  invoke<RequestRow[]>("get_ip_requests", { period, ip, limit, offset });

export const getIngestStatus = () => invoke<IngestStatus>("get_ingest_status");

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });

export const getAppInfo = () => invoke<AppInfo>("get_app_info");
export const replaceGeoip = (path: string) => invoke<AppInfo>("replace_geoip", { path });

/** 지운 해외 요청 행 수 */
export const purgeForeign = (beforeDate: string) => invoke<number>("purge_foreign", { beforeDate });

/** 다음 실행부터 적용 */
export const setDbPath = (path: string) => invoke<void>("set_db_path", { path });

export const onIngestProgress = (cb: (p: IngestProgress) => void): Promise<UnlistenFn> =>
  listen<IngestProgress>("ingest-progress", (e) => cb(e.payload));
