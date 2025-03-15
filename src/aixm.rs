use std::path::PathBuf;

use aixm::{Member, MessageAixmBasicMessage};
use itertools::Itertools as _;
use snafu::ResultExt as _;
use tokio::{
    fs::File,
    io::{AsyncReadExt as _, BufReader},
    sync::mpsc,
    task::JoinSet,
};
use tracing::error;

use crate::{AiracUpdaterResult, Message, OpenAixmSnafu, ReadAixmSnafu};

pub(crate) async fn load_aixm_files(tx: mpsc::Sender<Message>) -> AiracUpdaterResult<Vec<Member>> {
    let mut join_set = JoinSet::new();
    for file_path in &[
        "../sectors/aixm/ED_AirportHeliport_2025-02-20_2025-03-20_revision.xml",
        "../sectors/aixm/ED_Navaids_2025-02-20_2025-03-20_revision.xml",
        "../sectors/aixm/ED_Routes_2025-02-20_2025-03-20_revision.xml",
        "../sectors/aixm/ED_Runway_2025-02-20_2025-03-20_revision.xml",
        "../sectors/aixm/ED_Waypoints_2025-02-20_2025-03-20_revision.xml",
    ] {
        let path = PathBuf::from(file_path);
        join_set.spawn(load_aixm_file(path, tx.clone()));
    }

    Ok(join_set
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
            Ok(aixm) => Some(aixm),
        })
        .concat())
}

async fn load_aixm_file(
    file_path: PathBuf,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult<Vec<Member>> {
    let mut reader = BufReader::new(File::open(&file_path).await.context(OpenAixmSnafu {
        filename: file_path.clone(),
    })?);
    let mut buf = String::new();
    reader
        .read_to_string(&mut buf)
        .await
        .context(ReadAixmSnafu {
            filename: file_path.clone(),
        })?;
    let mut aixm_data = vec![];

    tx.send(Message::info(format!(
        "Loading AIXM: {}",
        file_path.display()
    )))
    .await?;

    if let Ok(aixm) = quick_xml::de::from_str::<MessageAixmBasicMessage>(&buf)
        .inspect_err(|e| eprintln!("{}: {e}", file_path.display()))
    {
        aixm_data.extend(aixm.message_has_member.into_iter().map(|m| m.member));
    }
    tx.send(Message::info(format!(
        "Loaded AIXM: {}",
        file_path.display()
    )))
    .await?;

    Ok(aixm_data)
}
