use std::path::PathBuf;
use std::time::Instant;

use chrono::{Duration, Local, NaiveDate};
use clap::Parser;
use loggen::{generate, GenConfig};

/// 가짜 Apache combined access 로그를 서버별 폴더로 만든다.
#[derive(Parser)]
struct Args {
    /// 출력 폴더
    #[arg(long)]
    out: PathBuf,
    #[arg(long, default_value_t = 16)]
    servers: u32,
    /// 끝 날짜(--end)로 끝나는 N일
    #[arg(long, default_value_t = 7)]
    days: u32,
    /// 마지막 날짜 YYYY-MM-DD. 기본은 어제
    #[arg(long)]
    end: Option<NaiveDate>,
    /// 서버 1대, 하루당 정상 요청 수
    #[arg(long = "per-day", default_value_t = 20000)]
    per_day: u32,
    /// 해외 IP 비율 0~1
    #[arg(long, default_value_t = 0.02)]
    foreign: f64,
    /// 404만 내는 스캐너 IP 수
    #[arg(long, default_value_t = 30)]
    scanners: u32,
    /// 마지막 날을 뺀 날짜의 파일을 .gz로 쓴다
    #[arg(long)]
    gzip: bool,
    #[arg(long, default_value_t = 1)]
    seed: u64,
}

fn main() -> std::io::Result<()> {
    let a = Args::parse();
    let end = a.end.unwrap_or_else(|| Local::now().date_naive() - Duration::days(1));
    let n = i64::from(a.days.max(1));
    let days = (0..n).map(|i| end - Duration::days(n - 1 - i)).collect();
    let started = Instant::now();
    let r = generate(&GenConfig {
        out_dir: a.out.clone(),
        servers: a.servers,
        days,
        requests_per_server_day: a.per_day,
        foreign_ratio: a.foreign,
        scanner_ips: a.scanners,
        gzip: a.gzip,
        seed: a.seed,
    })?;
    println!(
        "{}개 파일, {}줄(해외 {}줄, 404 IP {}개) -> {} ({:.1}초)",
        r.files.len(),
        r.total_lines,
        r.foreign_lines,
        r.not_found_ips,
        a.out.display(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
