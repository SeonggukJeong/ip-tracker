//! 현실적인 가짜 Apache combined access 로그 생성기.
//! 테스트, 화면 확인, DB 크기 실측에 쓴다. 같은 seed면 결과가 바이트 단위로 같다.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use chrono::{Datelike, NaiveDate};
use flate2::{Compression, GzBuilder};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

pub struct GenConfig {
    pub out_dir: PathBuf,
    pub servers: u32,
    pub days: Vec<NaiveDate>,
    pub requests_per_server_day: u32,
    pub foreign_ratio: f64,
    pub scanner_ips: u32,
    pub gzip: bool,
    pub seed: u64,
}

pub struct GenReport {
    pub files: Vec<PathBuf>,
    pub total_lines: u64,
    pub foreign_lines: u64,
    /// 기간 중 한 줄이라도 낸 스캐너 IP 수(404만 내므로 집계의 "404 제외 IP"와 같아야 한다)
    pub not_found_ips: u64,
}

/// (ISO2, /24 접두사 "a.b.c"). IP는 반드시 이 /24 안에서만 만든다.
pub const COUNTRY_PREFIXES: &[(&str, &str)] = &[
    ("KR", "211.234.200"),
    ("KR", "175.223.10"),
    ("KR", "121.140.10"),
    ("KR", "58.229.10"),
    ("US", "8.8.8"),
    ("US", "17.253.1"),
    ("US", "52.95.110"),
    ("JP", "210.130.1"),
    ("JP", "202.12.27"),
    ("JP", "133.242.1"),
    ("CN", "114.114.114"),
    ("CN", "223.5.5"),
    ("CN", "180.76.76"),
    ("VN", "203.113.131"),
    ("VN", "14.225.1"),
    ("DE", "85.214.1"),
    ("DE", "5.9.1"),
    ("CA", "142.112.1"),
    ("GB", "81.2.69"),
    ("FR", "80.12.1"),
    ("SG", "13.228.1"),
    ("AU", "1.128.1"),
    ("IN", "117.192.1"),
    ("BR", "200.160.2"),
];

/// 해외 국가별 비중. 미국·일본·중국이 크다.
fn weight(iso: &str) -> u32 {
    match iso {
        "US" => 25,
        "JP" => 15,
        "CN" => 15,
        _ => 5,
    }
}

/// 정상 IP는 .1~.199, 스캐너 IP는 .200~.254만 쓴다. 두 집단이 겹치지 않는다.
const NORMAL_LAST: std::ops::RangeInclusive<u8> = 1..=199;
const SCANNER_LAST: std::ops::RangeInclusive<u8> = 200..=254;

const API_PATHS: &[&str] = &[
    "/api/v1/users",
    "/api/v1/users/me",
    "/api/v1/orders",
    "/api/v1/orders/recent",
    "/api/v1/notices",
    "/api/v1/search",
    "/api/v1/files/download",
];
const SCAN_PATHS: &[&str] = &[
    "/wp-login.php",
    "/.env",
    "/phpmyadmin/",
    "/admin/login",
    "/.git/config",
    "/xmlrpc.php",
    "/cgi-bin/luci",
];
const AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.1 Safari/605.1.15",
    "Mozilla/5.0 (Linux; Android 13; SM-S918N) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Mobile Safari/537.36",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.1 Mobile/15E148 Safari/604.1",
    "okhttp/4.12.0",
];
const SCAN_AGENTS: &[&str] = &["curl/8.4.0", "python-requests/2.31.0", "Go-http-client/1.1", "-"];
const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

struct Pools {
    domestic: Vec<&'static str>,
    foreign: Vec<&'static str>,
}

fn pools() -> Pools {
    let mut p = Pools { domestic: Vec::new(), foreign: Vec::new() };
    for (iso, prefix) in COUNTRY_PREFIXES {
        if *iso == "KR" {
            p.domestic.push(prefix);
        } else {
            // 가중치만큼 넣어 균등 추첨이 곧 비중이 되게 한다
            for _ in 0..weight(iso) {
                p.foreign.push(prefix);
            }
        }
    }
    p
}

fn is_foreign(ip: &str) -> bool {
    !COUNTRY_PREFIXES.iter().any(|(iso, prefix)| *iso == "KR" && ip.starts_with(&format!("{prefix}.")))
}

fn make_ip(rng: &mut ChaCha8Rng, foreign: bool, pools: &Pools, last: std::ops::RangeInclusive<u8>) -> String {
    let list = if foreign { &pools.foreign } else { &pools.domestic };
    let prefix = list.choose(rng).unwrap();
    format!("{prefix}.{}", rng.gen_range(last))
}

struct Line {
    sec: u32,
    text: String,
}

fn format_line(date: NaiveDate, sec: u32, ip: &str, path: &str, status: u16, bytes: u32, agent: &str) -> Line {
    let text = format!(
        "{ip} - - [{:02}/{}/{}:{:02}:{:02}:{:02} +0900] \"GET {path} HTTP/1.1\" {status} {bytes} \"-\" \"{agent}\"",
        date.day(),
        MONTHS[date.month0() as usize],
        date.year(),
        sec / 3600,
        sec / 60 % 60,
        sec % 60,
    );
    Line { sec, text }
}

fn write_file(path: &PathBuf, lines: &[Line], gzip: bool) -> io::Result<()> {
    let file = BufWriter::new(File::create(path)?);
    if gzip {
        // mtime 0, OS 255(알 수 없음)로 고정해 결과가 항상 같다
        let mut w = GzBuilder::new().mtime(0).operating_system(255).write(file, Compression::default());
        for l in lines {
            writeln!(w, "{}", l.text)?;
        }
        w.finish()?.flush()
    } else {
        let mut w = file;
        for l in lines {
            writeln!(w, "{}", l.text)?;
        }
        w.flush()
    }
}

pub fn generate(cfg: &GenConfig) -> io::Result<GenReport> {
    let pools = pools();
    let mut report = GenReport { files: Vec::new(), total_lines: 0, foreign_lines: 0, not_found_ips: 0 };
    let last_day = cfg.days.iter().max().copied();

    // 스캐너는 기간 내내 같은 IP를 쓴다(해외 80%, 국내 20%).
    let mut srng = ChaCha8Rng::seed_from_u64(cfg.seed);
    let mut scanners: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    while (scanners.len() as u32) < cfg.scanner_ips {
        let foreign = srng.gen_bool(0.8);
        let ip = make_ip(&mut srng, foreign, &pools, SCANNER_LAST);
        if seen.insert(ip.clone()) {
            scanners.push(ip);
        }
    }
    let mut emitted: HashSet<String> = HashSet::new();

    for (day_idx, &date) in cfg.days.iter().enumerate() {
        let mut rng = ChaCha8Rng::seed_from_u64(cfg.seed);
        rng.set_stream(day_idx as u64 + 1);

        // 그날의 정상 IP 집단. 모든 서버가 이 집단을 나눠 쓴다(L4 분배).
        let total = u64::from(cfg.requests_per_server_day) * u64::from(cfg.servers);
        let pop_size = ((total / 10) as usize).max(20);
        let population: Vec<String> = (0..pop_size)
            .map(|_| {
                let foreign = rng.gen_bool(cfg.foreign_ratio.clamp(0.0, 1.0));
                make_ip(&mut rng, foreign, &pools, NORMAL_LAST)
            })
            .collect();

        for server in 1..=cfg.servers {
            let mut lines: Vec<Line> = Vec::with_capacity(cfg.requests_per_server_day as usize + 64);
            for _ in 0..cfg.requests_per_server_day {
                let ip = &population[rng.gen_range(0..population.len())];
                let path = API_PATHS[rng.gen_range(0..API_PATHS.len())];
                let agent = AGENTS[rng.gen_range(0..AGENTS.len())];
                lines.push(format_line(date, rng.gen_range(0..86400), ip, path, 200, rng.gen_range(200..60000), agent));
            }
            for ip in &scanners {
                if rng.gen_bool(0.5) {
                    emitted.insert(ip.clone());
                    for _ in 0..rng.gen_range(1..=4) {
                        let path = SCAN_PATHS[rng.gen_range(0..SCAN_PATHS.len())];
                        let agent = SCAN_AGENTS[rng.gen_range(0..SCAN_AGENTS.len())];
                        lines.push(format_line(date, rng.gen_range(0..86400), ip, path, 404, rng.gen_range(150..400), agent));
                    }
                }
            }
            // 로드밸런서 상태 점검(내부 IP)
            for _ in 0..(cfg.requests_per_server_day / 100).max(1) {
                lines.push(format_line(date, rng.gen_range(0..86400), "10.0.0.1", "/health", 200, 2, "ELB-HealthChecker/2.0"));
            }
            lines.sort_by_key(|l| l.sec); // 안정 정렬: 같은 초는 생성 순서

            report.total_lines += lines.len() as u64;
            report.foreign_lines += lines
                .iter()
                .filter(|l| {
                    let ip = l.text.split(' ').next().unwrap();
                    !ip.starts_with("10.") && is_foreign(ip)
                })
                .count() as u64;

            let gz = cfg.gzip && Some(date) != last_day;
            let dir = cfg.out_dir.join(format!("was{server:02}"));
            fs::create_dir_all(&dir)?;
            let name = format!("access_log.{}{}", date.format("%Y-%m-%d"), if gz { ".gz" } else { "" });
            let path = dir.join(name);
            write_file(&path, &lines, gz)?;
            report.files.push(path);
        }
    }
    report.not_found_ips = emitted.len() as u64;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn d(s: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn cfg(dir: &std::path::Path, servers: u32, days: &[&str], gzip: bool) -> GenConfig {
        GenConfig {
            out_dir: dir.to_path_buf(),
            servers,
            days: days.iter().map(|s| d(s)).collect(),
            requests_per_server_day: 200,
            foreign_ratio: 0.1,
            scanner_ips: 4,
            gzip,
            seed: 1,
        }
    }

    #[test]
    fn same_seed_same_output() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ra = generate(&cfg(a.path(), 2, &["2026-11-21", "2026-11-22"], true)).unwrap();
        let rb = generate(&cfg(b.path(), 2, &["2026-11-21", "2026-11-22"], true)).unwrap();
        assert_eq!(ra.files.len(), rb.files.len());
        for (fa, fb) in ra.files.iter().zip(&rb.files) {
            assert_eq!(fa.strip_prefix(a.path()).unwrap(), fb.strip_prefix(b.path()).unwrap());
            assert_eq!(fs::read(fa).unwrap(), fs::read(fb).unwrap());
        }
        assert_eq!(ra.total_lines, rb.total_lines);
    }

    #[test]
    fn different_seed_differs() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let ra = generate(&cfg(a.path(), 1, &["2026-11-22"], false)).unwrap();
        let mut c = cfg(b.path(), 1, &["2026-11-22"], false);
        c.seed = 2;
        let rb = generate(&c).unwrap();
        assert_ne!(fs::read(&ra.files[0]).unwrap(), fs::read(&rb.files[0]).unwrap());
    }

    #[test]
    fn layout_is_one_folder_per_server() {
        let dir = tempfile::tempdir().unwrap();
        let r = generate(&cfg(dir.path(), 3, &["2026-11-21", "2026-11-22"], false)).unwrap();
        assert_eq!(r.files.len(), 6);
        for s in ["was01", "was02", "was03"] {
            for day in ["2026-11-21", "2026-11-22"] {
                assert!(dir.path().join(s).join(format!("access_log.{day}")).is_file());
            }
        }
        assert!(!dir.path().join("was04").exists());
    }

    #[test]
    fn gzip_skips_last_day_and_lines_are_sorted_kst() {
        let dir = tempfile::tempdir().unwrap();
        generate(&cfg(dir.path(), 1, &["2026-11-21", "2026-11-22"], true)).unwrap();
        assert!(dir.path().join("was01/access_log.2026-11-21.gz").is_file());
        let text = fs::read_to_string(dir.path().join("was01/access_log.2026-11-22")).unwrap();
        let stamps: Vec<&str> = text
            .lines()
            .map(|l| l.split(" [").nth(1).unwrap().split("] ").next().unwrap())
            .collect();
        assert!(stamps.iter().all(|s| s.starts_with("22/Nov/2026:") && s.ends_with(" +0900")));
        assert!(stamps.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn scanners_only_404_and_normal_never_404() {
        let dir = tempfile::tempdir().unwrap();
        generate(&cfg(dir.path(), 2, &["2026-11-22"], false)).unwrap();
        let text = fs::read_to_string(dir.path().join("was01/access_log.2026-11-22")).unwrap();
        assert!(text.contains("10.0.0.1 "));
        let mut ips404 = std::collections::HashSet::new();
        let mut ips200 = std::collections::HashSet::new();
        for l in text.lines() {
            let ip = l.split(' ').next().unwrap();
            if l.contains("\" 404 ") { ips404.insert(ip) } else { ips200.insert(ip) };
        }
        assert!(!ips404.is_empty());
        assert!(ips404.is_disjoint(&ips200));
    }
}
