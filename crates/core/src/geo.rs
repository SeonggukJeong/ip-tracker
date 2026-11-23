use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::Path;

use chrono::{FixedOffset, NaiveDate};
use maxminddb::{path, Reader};

use crate::Error;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum OriginClass {
    Domestic = 0,
    Foreign = 1,
    Internal = 2,
    Unknown = 3,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Origin {
    pub class: OriginClass,
    /// ISO 3166-1 alpha-2, 대문자
    pub country: Option<String>,
}

pub struct GeoDb {
    reader: Reader<Vec<u8>>,
}

fn is_internal_v4(ip: Ipv4Addr) -> bool {
    // 10/8, 172.16/12, 192.168/16, 127/8, 169.254/16
    ip.is_private() || ip.is_loopback() || ip.is_link_local()
}

fn is_internal_v6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    ip.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
}

impl GeoDb {
    pub fn open(path: &Path) -> Result<GeoDb, Error> {
        Ok(GeoDb { reader: Reader::open_readfile(path)? })
    }

    pub fn classify(&self, ip: IpAddr) -> Origin {
        let ip = ip.to_canonical();
        let internal = match ip {
            IpAddr::V4(v4) => is_internal_v4(v4),
            IpAddr::V6(v6) => is_internal_v6(v6),
        };
        if internal {
            return Origin { class: OriginClass::Internal, country: None };
        }
        let code = self
            .reader
            .lookup(ip)
            .ok()
            .and_then(|r| r.decode_path::<String>(&path!["country", "iso_code"]).ok().flatten())
            .map(|s| s.to_ascii_uppercase())
            .filter(|s| !s.is_empty());
        match code {
            Some(c) if c == "KR" => Origin { class: OriginClass::Domestic, country: Some(c) },
            Some(c) => Origin { class: OriginClass::Foreign, country: Some(c) },
            None => Origin { class: OriginClass::Unknown, country: None },
        }
    }

    /// mmdb 메타데이터 build_epoch의 KST 날짜
    pub fn build_date(&self) -> NaiveDate {
        let kst = FixedOffset::east_opt(9 * 3600).unwrap();
        chrono::DateTime::from_timestamp(self.reader.metadata().build_epoch as i64, 0)
            .map(|t| t.with_timezone(&kst).date_naive())
            .unwrap_or(NaiveDate::MIN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> GeoDb {
        GeoDb::open(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src-tauri/resources/dbip-country-lite.mmdb"))).unwrap()
    }
    fn c(ip: &str) -> Origin { db().classify(ip.parse().unwrap()) }

    #[test] fn korean_ip_is_domestic() { assert_eq!(c("168.126.63.1"), Origin { class: OriginClass::Domestic, country: Some("KR".into()) }); }
    #[test] fn us_ip_is_foreign() { assert_eq!(c("8.8.8.8"), Origin { class: OriginClass::Foreign, country: Some("US".into()) }); }
    #[test] fn private_ranges_are_internal() {
        for ip in ["10.1.2.3", "172.16.0.1", "172.31.255.254", "192.168.0.10", "127.0.0.1", "169.254.1.1", "::1", "fd00::1", "fe80::1"] {
            assert_eq!(c(ip), Origin { class: OriginClass::Internal, country: None }, "{ip}");
        }
    }
    #[test] fn public_172_is_not_internal() { assert_ne!(c("172.32.0.1").class, OriginClass::Internal); }
    #[test] fn mapped_ipv6_uses_ipv4() { assert_eq!(c("::ffff:8.8.8.8"), c("8.8.8.8")); assert_eq!(c("::ffff:10.0.0.1").class, OriginClass::Internal); }
    #[test] fn build_date_is_recent() { assert!(db().build_date() >= chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()); }
    #[test] fn open_missing_file_is_error() { assert!(GeoDb::open(std::path::Path::new("/nonexistent.mmdb")).is_err()); }
}
