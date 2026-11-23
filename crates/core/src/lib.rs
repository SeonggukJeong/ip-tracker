pub mod geo;
pub mod logformat;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O 오류: {0}")]
    Io(#[from] std::io::Error),
    #[error("GeoIP 오류: {0}")]
    Geo(#[from] maxminddb::MaxMindDbError),
    #[error("{0}")]
    Other(String),
}
