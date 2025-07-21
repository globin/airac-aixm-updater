use snafu::ResultExt as _;
use std::path::Path;
use tokio::{fs::File, io::AsyncReadExt as _, sync::mpsc, task::JoinSet};
use tracing::{debug, error};
use vatsim_parser::isec::parse_isec_txt;
use vatsim_parser::prf::Prf;
use vatsim_parser::{ese::Ese, sct::Sct};

use crate::error::{
    AiracUpdaterResult, OpenEseSnafu, OpenIsecSnafu, OpenPrfSnafu, OpenSctSnafu, ParseEseSnafu,
    ParseIsecSnafu, ParsePrfSnafu, ParseSctSnafu, ReadEseSnafu, ReadIsecSnafu, ReadPrfSnafu,
    ReadSctSnafu,
};
use crate::{Message, aixm_combine::EuroscopeFile};

pub(crate) async fn load_euroscope_files(
    prf_path: &Path,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult<Vec<EuroscopeFile>> {
    let mut prf_contents = vec![];
    File::open(prf_path)
        .await
        .context(OpenPrfSnafu { filename: prf_path })?
        .read_to_end(&mut prf_contents)
        .await
        .context(ReadPrfSnafu { filename: prf_path })?;
    let prf = Prf::parse(prf_path, &prf_contents).context(ParsePrfSnafu { filename: prf_path })?;
    let mut join_handle = JoinSet::new();

    join_handle.spawn(handle_sct(prf.sct_path(), tx.clone()));
    join_handle.spawn(handle_ese(prf.ese_path(), tx.clone()));
    join_handle.spawn(handle_isec(prf.isec_path(), tx.clone()));

    Ok(join_handle
        .join_all()
        .await
        .into_iter()
        .filter_map(|res| match res {
            Err(e) => {
                if let Err(e) = tx.blocking_send(Message::error(e.to_string())) {
                    error!("{e}");
                }
                None
            }
            Ok(es_file) => Some(es_file),
        })
        .collect())
}

async fn handle_ese(
    filename: impl AsRef<Path>,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult<EuroscopeFile> {
    let filename = filename.as_ref();
    let mut buf = vec![];
    debug!("Opening .ese: {}", filename.display());
    let mut f = File::open(filename)
        .await
        .context(OpenEseSnafu { filename })?;

    tx.send(Message::info(format!(
        "Reading .ese: {}",
        filename.display()
    )))
    .await?;
    f.read_to_end(&mut buf)
        .await
        .context(ReadEseSnafu { filename })?;

    tx.send(Message::info(format!(
        "Parsing .ese: {}",
        filename.display()
    )))
    .await?;
    let ese = Ese::parse(&buf).context(ParseEseSnafu { filename })?;
    tx.send(Message::info(format!(
        "Parsing .ese complete: {}",
        filename.display()
    )))
    .await?;
    Ok(EuroscopeFile::Ese {
        path: filename.to_path_buf(),
        content: Box::new(ese),
    })
}

async fn handle_sct(
    filename: impl AsRef<Path>,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult<EuroscopeFile> {
    let filename = filename.as_ref();
    let mut buf = vec![];

    debug!("Opening .sct: {}", filename.display());
    let mut f = File::open(filename)
        .await
        .context(OpenSctSnafu { filename })?;

    tx.send(Message::info(format!(
        "Reading .sct: {}",
        filename.display()
    )))
    .await?;
    f.read_to_end(&mut buf)
        .await
        .context(ReadSctSnafu { filename })?;

    tx.send(Message::info(format!(
        "Parsing .sct: {}",
        filename.display()
    )))
    .await?;
    let sct = Sct::parse(&buf).context(ParseSctSnafu { filename })?;
    tx.send(Message::info(format!(
        "Parsing .sct complete: {} ({})",
        filename.display(),
        sct.info.name
    )))
    .await?;

    Ok(EuroscopeFile::Sct {
        path: filename.to_path_buf(),
        content: Box::new(sct),
    })
}

async fn handle_isec(
    filename: impl AsRef<Path>,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult<EuroscopeFile> {
    let filename = filename.as_ref();
    let mut buf = vec![];

    debug!("Opening isec.txt: {}", filename.display());
    let mut f = File::open(filename)
        .await
        .context(OpenIsecSnafu { filename })?;

    tx.send(Message::info(format!(
        "Reading isec.txt: {}",
        filename.display()
    )))
    .await?;
    f.read_to_end(&mut buf)
        .await
        .context(ReadIsecSnafu { filename })?;

    tx.send(Message::info(format!(
        "Parsing isec.txt: {}",
        filename.display()
    )))
    .await?;
    let isec = parse_isec_txt(&buf).context(ParseIsecSnafu { filename })?;
    tx.send(Message::info(format!(
        "Parsing isec.txt complete: {}",
        filename.display(),
    )))
    .await?;

    Ok(EuroscopeFile::Isec {
        path: filename.to_path_buf(),
        content: Box::new(isec),
    })
}
