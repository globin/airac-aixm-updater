use std::{fs::File, io::BufReader, path::PathBuf};

use aixm::MessageAixmBasicMessage;
use snafu::ResultExt as _;
use tokio::{sync::mpsc, task::JoinSet};

use crate::{AiracUpdaterResult, Message, OpenAixmSnafu};

pub(crate) fn load_aixm_files(
    join_set: &mut JoinSet<AiracUpdaterResult>,
    tx: mpsc::Sender<Message>,
) {
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
}

async fn load_aixm_file(file_path: PathBuf, tx: mpsc::Sender<Message>) -> AiracUpdaterResult {
    let reader = BufReader::new(File::open(&file_path).context(OpenAixmSnafu {
        filename: file_path.clone(),
    })?);
    let mut aixm_data = vec![];

    tx.send(Message::new(format!(
        "Loading AIXM: {}",
        file_path.display()
    )))
    .await?;

    if let Ok(aixm) = quick_xml::de::from_reader::<_, MessageAixmBasicMessage>(reader)
        .inspect_err(|e| eprintln!("{}: {e}", file_path.display()))
    {
        aixm_data.extend(aixm.message_has_member.into_iter().map(|m| m.member));
    }
    tx.send(Message::new(format!(
        "Loaded AIXM: {}",
        file_path.display()
    )))
    .await?;

    Ok(())
}
