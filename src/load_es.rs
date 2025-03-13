use snafu::{OptionExt as _, ResultExt as _};
use std::path::Path;
use tokio::{fs::File, io::AsyncReadExt as _, sync::mpsc, task::JoinSet};
use tracing::debug;
use vatsim_parser::{ese::Ese, sct::Sct};

use crate::{
    AiracUpdaterResult, Message, OpenEseSnafu, OpenSctSnafu, ParseEseSnafu, ParseSctSnafu,
    ProcessingSnafu, ReadDirSnafu, ReadEseSnafu, ReadSctSnafu,
};

pub(crate) fn load_euroscope_files(
    dir: &Path,
    join_handle: &mut JoinSet<AiracUpdaterResult>,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult {
    let files = dir.read_dir().context(ReadDirSnafu)?;
    files
        .filter_map(|f_result| {
            f_result.ok().and_then(|f| {
                let path = f.path();
                path.extension()
                    .is_some_and(|extension| extension == "sct" || extension == "ese")
                    .then_some(path)
            })
        })
        .for_each(|path| {
            join_handle.spawn(handle_file(path, tx.clone()));
        });

    Ok(())
}

async fn handle_file(file: impl AsRef<Path>, tx: mpsc::Sender<Message>) -> AiracUpdaterResult {
    let file = file.as_ref();
    let ext = file
        .extension()
        .context(ProcessingSnafu { filename: file })?;
    if ext == "ese" {
        handle_ese(file, tx.clone()).await?;
    }
    if ext == "sct" {
        handle_sct(file, tx.clone()).await?;
    }

    Ok(())
}

async fn handle_ese(filename: &Path, tx: mpsc::Sender<Message>) -> AiracUpdaterResult {
    let mut buf = vec![];
    debug!("Opening .ese: {}", filename.display());
    let mut f = File::open(filename)
        .await
        .context(OpenEseSnafu { filename })?;

    tx.send(Message::new(format!(
        "Reading .ese: {}",
        filename.display()
    )))
    .await?;
    f.read_to_end(&mut buf)
        .await
        .context(ReadEseSnafu { filename })?;

    tx.send(Message::new(format!(
        "Parsing .ese: {}",
        filename.display()
    )))
    .await?;
    let ese = Ese::parse(&buf).context(ParseEseSnafu { filename })?;
    tx.send(Message::new(format!(
        "Parsing .ese complete: {}",
        filename.display()
    )))
    .await?;
    Ok(())
}

async fn handle_sct(filename: &Path, tx: mpsc::Sender<Message>) -> AiracUpdaterResult {
    let mut buf = vec![];

    debug!("Opening .sct: {}", filename.display());
    let mut f = File::open(filename)
        .await
        .context(OpenSctSnafu { filename })?;

    tx.send(Message::new(format!(
        "Reading .sct: {}",
        filename.display()
    )))
    .await?;
    f.read_to_end(&mut buf)
        .await
        .context(ReadSctSnafu { filename })?;

    tx.send(Message::new(format!(
        "Parsing .sct: {}",
        filename.display()
    )))
    .await?;
    let sct = Sct::parse(&buf).context(ParseSctSnafu { filename })?;
    tx.send(Message::new(format!(
        "Parsing .sct complete: {} ({})",
        filename.display(),
        sct.info.name
    )))
    .await?;

    Ok(())
}
