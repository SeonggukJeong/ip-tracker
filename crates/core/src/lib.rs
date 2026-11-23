pub mod db;
pub mod geo;
pub mod ingest;
pub mod logformat;
pub mod query;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O 오류: {0}")]
    Io(#[from] std::io::Error),
    #[error("GeoIP 오류: {0}")]
    Geo(#[from] maxminddb::MaxMindDbError),
    #[error("DB 오류: {0}")]
    Db(#[from] rusqlite::Error),
    /// .gz를 풀다가 중간에 끊기거나 깨진 경우. 값은 파일명.
    #[error("압축 파일이 잘렸거나 손상됐습니다: {0}")]
    CorruptGz(String),
    #[error("{0}")]
    Other(String),
}
