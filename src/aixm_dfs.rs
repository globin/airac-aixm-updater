use serde::Deserialize;
use snafu::ResultExt as _;
use tracing::trace;

use crate::error::{
    AiracUpdaterResult, DecodeDfsDatasetsSnafu, DeserializeDfsDatasetsSnafu, FetchDfsDatasetsSnafu,
};

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct DfsAmdts {
    #[serde(rename = "Amdts")]
    amdts: Vec<DfsAmdt>,
}

#[derive(Debug, Deserialize, Clone)]
struct DfsAmdt {
    #[serde(rename = "Amdt")]
    amdt: u32,
    #[serde(rename = "Metadata")]
    metadata: DfsAmdtMetadata,
}

#[derive(Debug, Deserialize, Clone)]
struct DfsAmdtMetadata {
    datasets: Vec<DfsAmdtDataset>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
enum DfsAmdtDataset {
    #[serde(rename = "group")]
    Group {
        #[expect(dead_code, reason = "to be moved to public API of aixm lib")]
        name: String,
        items: Vec<DfsAmdtDataset>,
    },
    #[serde(rename = "leaf")]
    Leaf {
        name: String,
        releases: Vec<DfsAmdtDatasetRelease>,
    },
}

impl DfsAmdtDataset {
    fn find<F>(&self, predicate: &F) -> Option<&DfsAmdtDataset>
    where
        F: Fn(&DfsAmdtDataset) -> bool,
    {
        if predicate(self) {
            return Some(self);
        }

        if let DfsAmdtDataset::Group { name: _, items } = self {
            for item in items {
                if let Some(found) = item.find(predicate) {
                    return Some(found);
                }
            }
        }

        None
    }
}

#[derive(Debug, Deserialize, Clone)]
struct DfsAmdtDatasetRelease {
    #[serde(rename = "type")]
    release_type: String,
    filename: String,
}

pub(crate) async fn fetch_dfs_datasets() -> AiracUpdaterResult<DfsAmdts> {
    let raw_data = reqwest::get("https://aip.dfs.de/datasets/rest/")
        .await
        .context(FetchDfsDatasetsSnafu)?
        .text()
        .await
        .context(DecodeDfsDatasetsSnafu)?;
    trace!("{raw_data}");
    serde_json::from_str(&raw_data).context(DeserializeDfsDatasetsSnafu)
}

pub(crate) fn get_dataset_url(
    amdts: &DfsAmdts,
    amdt_id: u32,
    dataset_name: &str,
    release_type: &str,
) -> Option<String> {
    for amdt in &amdts.amdts {
        if amdt.amdt == amdt_id {
            for dataset in &amdt.metadata.datasets {
                if let Some(DfsAmdtDataset::Leaf { name: _, releases }) = dataset.find(&|d| matches!(d, DfsAmdtDataset::Leaf{ name, releases: _} if name == dataset_name)) {
                    for r in releases {
                        if r.release_type == release_type {
                            return Some(format!("https://aip.dfs.de/datasets/rest/{}/{}", amdt_id, r.filename));
                        }
                    }
                }
            }
        }
    }

    None
}
