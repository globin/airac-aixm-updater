use aixm::{Member, MessageAixmBasicMessage};
use itertools::Itertools as _;
use quick_xml::DeError;
use snafu::{OptionExt, ResultExt as _};
use tokio::{
    sync::mpsc,
    task::{JoinSet, spawn_blocking},
};
use tracing::error;

use crate::{
    AiracUpdaterResult, DatasetNotFoundSnafu, DecodeDatasetSnafu, DeserializeDatasetSnafu,
    FetchDatasetSnafu, Message,
    aixm_dfs::{fetch_dfs_datasets, get_dataset_url},
};

pub(crate) async fn load_aixm_files(tx: mpsc::Sender<Message>) -> AiracUpdaterResult<Vec<Member>> {
    let mut join_set = JoinSet::new();
    let dataset_metadata = fetch_dfs_datasets().await?;
    for dataset in &[
        "ED AirportHeliport",
        "ED Navaids",
        "ED Routes",
        "ED Runway",
        "ED Waypoints",
        // "../sectors/aixm/ED_AirportHeliport_2025-02-20_2025-03-20_revision.xml",
        // "../sectors/aixm/ED_Navaids_2025-02-20_2025-03-20_revision.xml",
        // "../sectors/aixm/ED_Routes_2025-02-20_2025-03-20_revision.xml",
        // "../sectors/aixm/ED_Runway_2025-02-20_2025-03-20_revision.xml",
        // "../sectors/aixm/ED_Waypoints_2025-02-20_2025-03-20_revision.xml",
    ] {
        // let path = PathBuf::from(file_path);
        // join_set.spawn(load_aixm_file(path, tx.clone()));

        let dataset_url = get_dataset_url(&dataset_metadata, 0, dataset, "AIXM 5.1").context(
            DatasetNotFoundSnafu {
                dataset: (*dataset).to_string(),
            },
        )?;
        join_set.spawn(fetch_and_load_dfs_dataset(dataset_url, dataset, tx.clone()));
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

async fn fetch_and_load_dfs_dataset(
    dataset_url: impl AsRef<str>,
    dataset_name: &str,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult<Vec<Member>> {
    tx.send(Message::info(format!("Fetching AIXM: {dataset_name}")))
        .await?;
    let data = reqwest::get(dataset_url.as_ref())
        .await
        .context(FetchDatasetSnafu {
            dataset: dataset_name.to_string(),
        })?
        .bytes()
        .await
        .context(DecodeDatasetSnafu {
            dataset: dataset_name.to_string(),
        })?;
    tx.send(Message::info(format!("Fetched AIXM: {dataset_name}")))
        .await?;
    load_aixm_data(data.to_vec(), dataset_name, tx.clone()).await
}

async fn load_aixm_data(
    data: Vec<u8>,
    dataset: &str,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult<Vec<Member>> {
    tx.send(Message::info(format!("Loading AIXM: {dataset}",)))
        .await?;

    let aixm_data = spawn_blocking(move || {
        Ok::<_, DeError>(
            quick_xml::de::from_reader::<_, MessageAixmBasicMessage>(&*data)?
                .message_has_member
                .into_iter()
                .map(|m| m.member)
                .collect(),
        )
    })
    .await?
    .context(DeserializeDatasetSnafu {
        dataset: dataset.to_string(),
    });
    tx.send(Message::info(format!("Loaded AIXM: {dataset}",)))
        .await?;

    aixm_data
}
