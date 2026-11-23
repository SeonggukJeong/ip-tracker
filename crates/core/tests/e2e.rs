//! loggen이 만든 로그를 core로 적재·집계하는 통합 검사.

use std::path::PathBuf;

use chrono::NaiveDate;
use iptracker_core::db::open_in_memory;
use iptracker_core::geo::{GeoDb, OriginClass};
use iptracker_core::ingest::{
    detect_file, ingest_file, scan, ConflictPolicy, IngestOptions, IngestOutcome,
};
use iptracker_core::query::{ingest_status, kpis, Filter, Period};
use loggen::{generate, GenConfig, COUNTRY_PREFIXES};
use rusqlite::Connection;

fn mmdb() -> GeoDb {
    GeoDb::open(&PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../src-tauri/resources/dbip-country-lite.mmdb"
    )))
    .unwrap()
}

fn p(from: &str, to: &str) -> Period {
    Period { from: from.into(), to: to.into() }
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

#[test]
fn country_samples_match_bundled_geoip() {
    let geo = mmdb();
    let mut bad = Vec::new();
    for (iso, prefix) in COUNTRY_PREFIXES {
        for last in [1, 254] {
            let ip = format!("{prefix}.{last}").parse().unwrap();
            let o = geo.classify(ip);
            let want = if *iso == "KR" { OriginClass::Domestic } else { OriginClass::Foreign };
            if o.class != want || o.country.as_deref() != Some(*iso) {
                bad.push(format!("{iso} {prefix}.{last} -> {o:?}"));
            }
        }
    }
    assert!(bad.is_empty(), "내장 DB와 맞지 않는 접두사: {bad:#?}");
    for iso in ["US", "JP", "CN", "VN", "DE", "CA", "GB", "FR", "SG", "AU", "IN", "BR", "KR"] {
        assert!(COUNTRY_PREFIXES.iter().any(|(c, _)| *c == iso), "{iso} 없음");
    }
}

#[test]
fn generated_logs_ingest_and_query_consistently() {
    let dir = tempfile::tempdir().unwrap();
    let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    let report = generate(&GenConfig {
        out_dir: dir.path().to_path_buf(),
        servers: 4,
        days: vec![d("2026-11-21"), d("2026-11-22")],
        requests_per_server_day: 500,
        foreign_ratio: 0.1,
        scanner_ips: 5,
        gzip: true,
        seed: 7,
    })
    .unwrap();
    assert_eq!(report.files.len(), 8);

    let geo = mmdb();
    let mut conn = open_in_memory().unwrap();
    let opts = IngestOptions { conflict: ConflictPolicy::Ask, allow_high_failure: false };
    let cands = scan(&[dir.path().to_path_buf()], None);
    assert_eq!(cands.len(), 8);
    for c in &cands {
        let parser = detect_file(&c.path, None).unwrap().expect("형식 인식");
        let out = ingest_file(&mut conn, &geo, &parser, c, &opts, &mut |_, _| {}).unwrap();
        assert!(matches!(out, IngestOutcome::Ingested(_)), "{c:?} -> {out:?}");
    }

    let f = Filter { period: p("2026-11-21", "2026-11-22"), exclude_404: false };
    let k = kpis(&conn, &f).unwrap();
    assert_eq!(count(&conn, "ingest_files"), 8);
    assert_eq!(count(&conn, "foreign_requests") as u64, report.foreign_lines);
    assert_eq!(k.cur.foreign_requests, report.foreign_lines);
    assert_eq!(k.excluded_ips, report.not_found_ips);
    assert!(k.cur.countries >= 3);

    // 같은 폴더를 다시 넣으면 전부 Duplicate, 숫자 변화 없음
    for c in &cands {
        let parser = detect_file(&c.path, None).unwrap().unwrap();
        let out = ingest_file(&mut conn, &geo, &parser, c, &opts, &mut |_, _| {}).unwrap();
        assert_eq!(out, IngestOutcome::Duplicate);
    }
    assert_eq!(kpis(&conn, &f).unwrap(), k);
    assert_eq!(count(&conn, "ingest_files"), 8);

    let k_ex = kpis(&conn, &Filter { exclude_404: true, ..f }).unwrap();
    assert!(k_ex.cur.total_requests < k.cur.total_requests);
    assert_eq!(ingest_status(&conn).unwrap().servers_on_latest, 4);
}
