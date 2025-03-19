use std::path::PathBuf;

use snafu::Snafu;
use tokio::{sync::mpsc::error::SendError, task::JoinError};
use vatsim_parser::{ese::EseError, sct::SctError};

use crate::Message;

pub(crate) type AiracUpdaterResult<T = ()> = Result<T, Error>;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub(crate) enum Error {
    #[snafu(display("Could not process {}", filename.display()))]
    Processing { filename: PathBuf },

    #[snafu(display("Could not read directory: {source}"))]
    ReadDir { source: std::io::Error },

    #[snafu(display("Could not rename file ({} -> {}): {source}", from.display(), to.display()))]
    Rename {
        source: std::io::Error,
        from: PathBuf,
        to: PathBuf,
    },
    #[snafu(display("Could not create file ({}): {source}", path.display()))]
    CreateNew {
        source: std::io::Error,
        path: PathBuf,
    },
    #[snafu(display("Could not write to new file ({}): {source}", path.display()))]
    WriteNew {
        source: std::io::Error,
        path: PathBuf,
    },

    #[snafu(display("Could not deserialize DFS AIXM dataset list: {source}"))]
    DeserializeDfsDatasets { source: serde_json::Error },

    #[snafu(display("Could not decode DFS AIXM dataset list: {source}"))]
    DecodeDfsDatasets { source: reqwest::Error },

    #[snafu(display("Could not fetch DFS AIXM dataset list: {source}"))]
    FetchDfsDatasets { source: reqwest::Error },

    #[snafu(display("Could not find AIXM dataset ({dataset})"))]
    DatasetNotFound { dataset: String },

    #[snafu(display("Could not deserialize AIXM dataset ({dataset}): {source}"))]
    DeserializeDataset {
        dataset: String,
        source: quick_xml::DeError,
    },

    #[snafu(display("Could not decode AIXM dataset ({dataset}): {source}"))]
    DecodeDataset {
        dataset: String,
        source: reqwest::Error,
    },

    #[snafu(display("Could not fetch AIXM dataset ({dataset}): {source}"))]
    FetchDataset {
        dataset: String,
        source: reqwest::Error,
    },

    #[snafu(display("Could not read AIXM ({}): {source}", filename.display()))]
    #[expect(dead_code, reason = "to be used for local AIXM data")]
    ReadAixm {
        filename: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Could not open AIXM ({}): {source}", filename.display()))]
    #[expect(dead_code, reason = "to be used for local AIXM data")]
    OpenAixm {
        filename: PathBuf,
        source: std::io::Error,
    },

    #[snafu(display("Could not find EuroScope controller pack: {}", directory.display()))]
    NoEuroscopePackFound { directory: PathBuf },

    #[snafu(display("Could not open .ese ({}): {source}", filename.display()))]
    OpenEse {
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Could not read .ese ({}): {source}", filename.display()))]
    ReadEse {
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Could not parse .ese ({}): {source}", filename.display()))]
    ParseEse {
        filename: PathBuf,
        #[snafu(source(from(EseError, Box::new)))]
        source: Box<EseError>,
    },

    #[snafu(display("Could not open .sct ({}): {source}", filename.display()))]
    OpenSct {
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Could not read .sct ({}): {source}", filename.display()))]
    ReadSct {
        filename: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("Could not parse .sct ({}): {source}", filename.display()))]
    ParseSct {
        filename: PathBuf,
        #[snafu(source(from(SctError, Box::new)))]
        source: Box<SctError>,
    },

    #[snafu(context(false))]
    Send { source: SendError<Message> },

    #[snafu(context(false))]
    Join { source: JoinError },
}
