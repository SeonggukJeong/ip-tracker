//! 대시보드 집계 질의.

use std::collections::{HashMap, HashSet};

use chrono::{Duration, NaiveDate};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

use crate::Error;

/// "YYYY-MM-DD", 양끝 포함
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Period {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub period: Period,
    pub exclude_404: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodStats {
    pub total_requests: u64,
    pub foreign_requests: u64,
    pub foreign_ips: u64,
    pub domestic_ips: u64,
    pub countries: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kpis {
    pub cur: PeriodStats,
    /// 바로 앞의 같은 길이 기간
    pub prev: PeriodStats,
    /// 스위치와 무관하게 센, 404를 낸 해외 IP 수
    pub foreign_ips_404: u64,
    /// 스위치와 무관하게 센, 404를 낸 국내+해외 IP 수
    pub excluded_ips: u64,
    /// prev에는 없고 cur에는 있는 해외 국가 수
    pub new_countries: u64,
    /// 분류 "알 수 없음" 요청 수
    pub unknown_requests: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountryRow {
    pub country: String,
    pub requests: u64,
    /// 해외 요청 중 비율 0~1
    pub share: f64,
    pub ips: u64,
    pub not_found: u64,
    pub ips_404: u64,
    pub last_ts: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayPoint {
    pub date: String,
    pub foreign_requests: u64,
    pub foreign_ips: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IpRow {
    pub ip: String,
    pub requests: u64,
    pub not_found: u64,
    pub first_ts: i64,
    pub last_ts: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestRow {
    pub ts: i64,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub bytes: Option<u64>,
    pub user_agent: Option<String>,
    pub server: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestStatus {
    pub latest_date: Option<String>,
    pub servers_on_latest: u32,
    pub file_count: u32,
}

/// ?1=from, ?2=to, ?3=exclude_404(0/1)를 쓰는 조건. 기간 안에서 404를 낸 IP의 행을 뺀다.
const NOT_EXCLUDED: &str = "(?3 = 0 OR ip NOT IN (SELECT ip FROM ip_daily \
     WHERE date BETWEEN ?1 AND ?2 AND class IN (0,1) GROUP BY ip HAVING SUM(not_found) > 0))";

/// SQLite는 정수를 i64로만 돌려준다. 집계 값은 음수가 될 수 없다.
fn get_u64(r: &Row, i: usize) -> rusqlite::Result<u64> {
    Ok(r.get::<_, i64>(i)?.max(0) as u64)
}

fn parse_date(s: &str) -> Result<NaiveDate, Error> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| Error::Other(format!("날짜 형식 오류 {s:?}: {e}")))
}

fn fmt_date(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// period 바로 앞의 같은 일수 기간.
fn prev_period(p: &Period) -> Result<Period, Error> {
    let from = parse_date(&p.from)?;
    let to = parse_date(&p.to)?;
    let days = (to - from).num_days() + 1;
    Ok(Period { from: fmt_date(from - Duration::days(days)), to: fmt_date(from - Duration::days(1)) })
}

fn period_stats(conn: &Connection, p: &Period, exclude: bool) -> Result<PeriodStats, Error> {
    let sql = format!(
        "SELECT COALESCE(SUM(requests),0), \
                COALESCE(SUM(CASE WHEN class=1 THEN requests END),0), \
                COUNT(DISTINCT CASE WHEN class=1 THEN ip END), \
                COUNT(DISTINCT CASE WHEN class=0 THEN ip END), \
                COUNT(DISTINCT CASE WHEN class=1 THEN country END) \
         FROM ip_daily WHERE date BETWEEN ?1 AND ?2 AND class IN (0,1) AND {NOT_EXCLUDED}"
    );
    Ok(conn.query_row(&sql, params![p.from, p.to, exclude as i64], |r| {
        Ok(PeriodStats {
            total_requests: get_u64(r, 0)?,
            foreign_requests: get_u64(r, 1)?,
            foreign_ips: get_u64(r, 2)?,
            domestic_ips: get_u64(r, 3)?,
            countries: get_u64(r, 4)?,
        })
    })?)
}

fn foreign_countries(conn: &Connection, p: &Period, exclude: bool) -> Result<HashSet<String>, Error> {
    let sql = format!(
        "SELECT DISTINCT country FROM ip_daily \
         WHERE date BETWEEN ?1 AND ?2 AND class = 1 AND country IS NOT NULL AND {NOT_EXCLUDED}"
    );
    let mut st = conn.prepare(&sql)?;
    let rows = st.query_map(params![p.from, p.to, exclude as i64], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn kpis(conn: &Connection, f: &Filter) -> Result<Kpis, Error> {
    let prev_p = prev_period(&f.period)?;
    let cur = period_stats(conn, &f.period, f.exclude_404)?;
    let prev = period_stats(conn, &prev_p, f.exclude_404)?;
    let (foreign_ips_404, excluded_ips, unknown_requests) = conn.query_row(
        "WITH per_ip AS (SELECT ip, class, SUM(not_found) AS nf, SUM(requests) AS req FROM ip_daily \
                         WHERE date BETWEEN ?1 AND ?2 GROUP BY ip, class) \
         SELECT COALESCE(SUM(class=1 AND nf>0),0), COALESCE(SUM(class IN (0,1) AND nf>0),0), \
                COALESCE(SUM(CASE WHEN class=3 THEN req END),0) FROM per_ip",
        params![f.period.from, f.period.to],
        |r| Ok((get_u64(r, 0)?, get_u64(r, 1)?, get_u64(r, 2)?)),
    )?;
    let prev_countries = foreign_countries(conn, &prev_p, f.exclude_404)?;
    let new_countries = foreign_countries(conn, &f.period, f.exclude_404)?
        .difference(&prev_countries)
        .count() as u64;
    Ok(Kpis { cur, prev, foreign_ips_404, excluded_ips, new_countries, unknown_requests })
}

pub fn countries(conn: &Connection, f: &Filter) -> Result<Vec<CountryRow>, Error> {
    let sql = format!(
        "WITH per_ip AS (SELECT country, ip, SUM(requests) AS req, SUM(not_found) AS nf, MAX(last_ts) AS last_ts \
                         FROM ip_daily WHERE date BETWEEN ?1 AND ?2 AND class = 1 AND country IS NOT NULL \
                         AND {NOT_EXCLUDED} GROUP BY country, ip) \
         SELECT country, SUM(req), COUNT(*), SUM(nf), SUM(nf > 0), MAX(last_ts) \
         FROM per_ip GROUP BY country ORDER BY SUM(req) DESC, country"
    );
    let mut st = conn.prepare(&sql)?;
    let mut rows = st
        .query_map(params![f.period.from, f.period.to, f.exclude_404 as i64], |r| {
            Ok(CountryRow {
                country: r.get(0)?,
                requests: get_u64(r, 1)?,
                share: 0.0,
                ips: get_u64(r, 2)?,
                not_found: get_u64(r, 3)?,
                ips_404: get_u64(r, 4)?,
                last_ts: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let total: u64 = rows.iter().map(|r| r.requests).sum();
    if total > 0 {
        for r in &mut rows {
            r.share = r.requests as f64 / total as f64;
        }
    }
    Ok(rows)
}

pub fn daily(conn: &Connection, f: &Filter) -> Result<Vec<DayPoint>, Error> {
    let sql = format!(
        "SELECT date, SUM(requests), COUNT(DISTINCT ip) FROM ip_daily \
         WHERE date BETWEEN ?1 AND ?2 AND class = 1 AND {NOT_EXCLUDED} GROUP BY date"
    );
    let mut st = conn.prepare(&sql)?;
    let found = st
        .query_map(params![f.period.from, f.period.to, f.exclude_404 as i64], |r| {
            Ok((r.get::<_, String>(0)?, (get_u64(r, 1)?, get_u64(r, 2)?)))
        })?
        .collect::<Result<HashMap<_, _>, _>>()?;
    let to = parse_date(&f.period.to)?;
    let mut day = parse_date(&f.period.from)?;
    let mut out = Vec::new();
    while day <= to {
        let date = fmt_date(day);
        let (foreign_requests, foreign_ips) = found.get(&date).copied().unwrap_or((0, 0));
        out.push(DayPoint { date, foreign_requests, foreign_ips });
        day += Duration::days(1);
    }
    Ok(out)
}

pub fn country_ips(
    conn: &Connection,
    f: &Filter,
    country: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<IpRow>, Error> {
    let sql = format!(
        "SELECT ip, SUM(requests) AS req, SUM(not_found), MIN(first_ts), MAX(last_ts) FROM ip_daily \
         WHERE date BETWEEN ?1 AND ?2 AND class = 1 AND country = ?4 AND {NOT_EXCLUDED} \
         GROUP BY ip ORDER BY req DESC, ip LIMIT ?5 OFFSET ?6"
    );
    let mut st = conn.prepare(&sql)?;
    let rows = st.query_map(
        params![f.period.from, f.period.to, f.exclude_404 as i64, country, limit, offset],
        |r| {
            Ok(IpRow { ip: r.get(0)?, requests: get_u64(r, 1)?, not_found: get_u64(r, 2)?, first_ts: r.get(3)?, last_ts: r.get(4)? })
        },
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn ip_requests(
    conn: &Connection,
    period: &Period,
    ip: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<RequestRow>, Error> {
    let mut st = conn.prepare(
        "SELECT fr.ts, fr.method, fr.url, fr.status, fr.bytes, ua.ua, f.server \
         FROM foreign_requests fr \
         JOIN ingest_files f ON f.id = fr.file_id \
         LEFT JOIN user_agents ua ON ua.id = fr.ua_id \
         WHERE fr.ip = ?1 AND fr.date BETWEEN ?2 AND ?3 \
         ORDER BY fr.ts DESC, fr.id DESC LIMIT ?4 OFFSET ?5",
    )?;
    let rows = st.query_map(params![ip, period.from, period.to, limit, offset], |r| {
        Ok(RequestRow {
            ts: r.get(0)?,
            method: r.get(1)?,
            url: r.get(2)?,
            status: r.get(3)?,
            bytes: r.get::<_, Option<i64>>(4)?.map(|b| b.max(0) as u64),
            user_agent: r.get(5)?,
            server: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn ingest_status(conn: &Connection) -> Result<IngestStatus, Error> {
    let file_count: u32 = conn.query_row("SELECT COUNT(*) FROM ingest_files", [], |r| r.get(0))?;
    let latest_date: Option<String> = conn.query_row("SELECT MAX(date) FROM ip_daily", [], |r| r.get(0))?;
    let servers_on_latest: u32 = match &latest_date {
        Some(d) => conn.query_row(
            "SELECT COUNT(DISTINCT f.server) FROM ingest_files f \
             WHERE f.id IN (SELECT file_id FROM ip_daily WHERE date = ?1)",
            [d],
            |r| r.get(0),
        )?,
        None => 0,
    };
    Ok(IngestStatus { latest_date, servers_on_latest, file_count })
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    type Row = (&'static str, &'static str, &'static str, u8, Option<&'static str>, u64, u64);

    const BASE: &[Row] = &[
        ("2026-11-22", "was01", "168.126.63.1", 0, Some("KR"), 100, 0),
        ("2026-11-22", "was02", "168.126.63.1", 0, Some("KR"), 50, 0),
        ("2026-11-22", "was01", "168.126.63.2", 0, Some("KR"), 10, 2),
        ("2026-11-22", "was01", "8.8.8.8", 1, Some("US"), 30, 0),
        ("2026-11-21", "was01", "8.8.8.8", 1, Some("US"), 10, 0),
        ("2026-11-22", "was01", "1.0.0.1", 1, Some("JP"), 5, 5),
        ("2026-11-22", "was01", "10.0.0.1", 2, None, 999, 0),
        ("2026-11-22", "was01", "203.0.113.1", 3, None, 7, 0),
        ("2026-11-19", "was01", "9.9.9.9", 1, Some("DE"), 4, 0),
    ];

    fn period(from: &str, to: &str) -> Period {
        Period { from: from.into(), to: to.into() }
    }
    fn p() -> Period {
        period("2026-11-21", "2026-11-22")
    }
    fn f(period: Period, exclude_404: bool) -> Filter {
        Filter { period, exclude_404 }
    }

    /// 서버마다 ingest_files 행 하나를 만들고 ip_daily에 직접 넣는다.
    fn seed(conn: &Connection, rows: &[Row]) {
        let mut ids = std::collections::HashMap::new();
        for (date, server, ip, class, country, requests, not_found) in rows {
            let id = *ids.entry(*server).or_insert_with(|| {
                conn.execute(
                    "INSERT INTO ingest_files(server, filename, sha256, lines, failed_lines, ingested_at) \
                     VALUES (?1, ?2, 'x', 0, 0, 0)",
                    params![server, format!("access-{server}.log")],
                )
                .unwrap();
                conn.last_insert_rowid()
            });
            conn.execute(
                "INSERT INTO ip_daily(date, file_id, ip, class, country, requests, not_found, first_ts, last_ts) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1000, 2000)",
                params![date, id, ip, class, country, *requests as i64, *not_found as i64],
            )
            .unwrap();
        }
    }

    fn base() -> Connection {
        let c = db::open_in_memory().unwrap();
        seed(&c, BASE);
        c
    }

    #[test]
    fn kpis_without_exclusion() {
        let c = base();
        let k = kpis(&c, &f(p(), false)).unwrap();
        assert_eq!(k.cur, PeriodStats { total_requests: 205, foreign_requests: 45, foreign_ips: 2, domestic_ips: 2, countries: 2 });
        assert_eq!(k.prev, PeriodStats { total_requests: 4, foreign_requests: 4, foreign_ips: 1, domestic_ips: 0, countries: 1 });
        assert_eq!((k.foreign_ips_404, k.excluded_ips, k.new_countries, k.unknown_requests), (1, 2, 2, 7));
    }

    #[test]
    fn kpis_with_exclusion_drops_404_ips_on_both_sides() {
        let c = base();
        let k = kpis(&c, &f(p(), true)).unwrap();
        assert_eq!(k.cur, PeriodStats { total_requests: 190, foreign_requests: 40, foreign_ips: 1, domestic_ips: 1, countries: 1 });
        assert_eq!((k.foreign_ips_404, k.excluded_ips), (1, 2));
    }

    #[test]
    fn countries_rows() {
        let c = base();
        let v = countries(&c, &f(p(), false)).unwrap();
        assert_eq!(
            v.iter().map(|r| (r.country.as_str(), r.requests, r.ips, r.not_found, r.ips_404)).collect::<Vec<_>>(),
            vec![("US", 40, 1, 0, 0), ("JP", 5, 1, 5, 1)]
        );
        assert!((v[0].share - 40.0 / 45.0).abs() < 1e-9);
        assert_eq!(v[0].last_ts, 2000);
        assert_eq!(countries(&c, &f(p(), true)).unwrap().len(), 1);
    }

    #[test]
    fn daily_fills_missing_dates_with_zero() {
        let c = base();
        let v = daily(&c, &f(period("2026-11-20", "2026-11-22"), false)).unwrap();
        assert_eq!(
            v.iter().map(|d| (d.date.as_str(), d.foreign_requests, d.foreign_ips)).collect::<Vec<_>>(),
            vec![("2026-11-20", 0, 0), ("2026-11-21", 10, 1), ("2026-11-22", 35, 2)]
        );
    }

    #[test]
    fn country_ips_merges_days_and_servers() {
        let c = base();
        let v = country_ips(&c, &f(p(), false), "US", 50, 0).unwrap();
        assert_eq!((v.len(), v[0].ip.as_str(), v[0].requests), (1, "8.8.8.8", 40));
    }

    #[test]
    fn empty_db_returns_zeros() {
        let c = db::open_in_memory().unwrap();
        let k = kpis(&c, &f(p(), true)).unwrap();
        assert_eq!(k.cur, PeriodStats { total_requests: 0, foreign_requests: 0, foreign_ips: 0, domestic_ips: 0, countries: 0 });
        assert!(countries(&c, &f(p(), false)).unwrap().is_empty());
        assert_eq!(daily(&c, &f(p(), false)).unwrap().len(), 2);
        let s = ingest_status(&c).unwrap();
        assert_eq!((s.latest_date, s.servers_on_latest, s.file_count), (None, 0, 0));
    }

    #[test]
    fn ingest_status_counts_servers_on_latest_date() {
        let c = base();
        let s = ingest_status(&c).unwrap();
        assert_eq!((s.latest_date.as_deref(), s.servers_on_latest), (Some("2026-11-22"), 2));
    }

    #[test]
    fn ip_requests_are_newest_first_with_server_name() {
        let c = base();
        let file_id: i64 = c.query_row("SELECT id FROM ingest_files WHERE server='was01'", [], |r| r.get(0)).unwrap();
        for ts in [100, 200] {
            c.execute(
                "INSERT INTO foreign_requests(ts, date, ip, country, file_id, method, url, status) \
                 VALUES (?1, '2026-11-22', '8.8.8.8', 'US', ?2, 'GET', '/', 200)",
                params![ts, file_id],
            )
            .unwrap();
        }
        let v = ip_requests(&c, &p(), "8.8.8.8", 10, 0).unwrap();
        assert_eq!(v.iter().map(|r| (r.ts, r.server.as_str())).collect::<Vec<_>>(), vec![(200, "was01"), (100, "was01")]);
    }
}
