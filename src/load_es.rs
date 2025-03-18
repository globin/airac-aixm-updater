use snafu::{OptionExt as _, ResultExt as _};
use std::path::Path;
use tokio::{fs::File, io::AsyncReadExt as _, sync::mpsc, task::JoinSet};
use tracing::{debug, error};
use vatsim_parser::{ese::Ese, sct::Sct};

use crate::{
    AiracUpdaterResult, Message, NoEuroscopePackFoundSnafu, OpenEseSnafu, OpenSctSnafu,
    ParseEseSnafu, ParseSctSnafu, ProcessingSnafu, ReadDirSnafu, ReadEseSnafu, ReadSctSnafu,
    aixm_combine::EuroscopeFile,
};

pub(crate) async fn load_euroscope_files(
    dir: &Path,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult<Vec<EuroscopeFile>> {
    let mut join_handle = JoinSet::new();
    let files = dir
        .read_dir()
        .context(ReadDirSnafu)?
        .filter_map(|f_result| {
            f_result.ok().and_then(|f| {
                let path = f.path();
                path.extension()
                    .is_some_and(|extension| extension == "sct" || extension == "ese")
                    .then_some(path)
            })
        })
        .collect::<Vec<_>>();

    if files.is_empty() {
        return NoEuroscopePackFoundSnafu {
            directory: dir.to_path_buf(),
        }
        .fail();
    }

    for path in files {
        join_handle.spawn(handle_file(path, tx.clone()));
    }

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

async fn handle_file(
    file: impl AsRef<Path>,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult<EuroscopeFile> {
    let file = file.as_ref();
    let ext = file
        .extension()
        .context(ProcessingSnafu { filename: file })?;
    match ext {
        _ if ext == "ese" => handle_ese(file, tx.clone()).await,
        _ if ext == "sct" => handle_sct(file, tx.clone()).await,
        _ => unreachable!(
            "Should never try to process file extension {}",
            ext.to_string_lossy()
        ),
    }
}

async fn handle_ese(
    filename: &Path,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult<EuroscopeFile> {
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
    filename: &Path,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult<EuroscopeFile> {
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
