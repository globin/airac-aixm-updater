mod isec;
mod sct;

use std::path::{Path, PathBuf};

use aixm::Member;
use chrono::Utc;
use snafu::ResultExt as _;
use tokio::{fs::OpenOptions, io::AsyncWriteExt, sync::mpsc};
use vatsim_parser::{ese::Ese, isec::IsecMap, sct::Sct};

use crate::{
    Message,
    error::{AiracUpdaterResult, CreateNewSnafu, RenameSnafu, WriteNewSnafu},
};

pub trait AixmUpdateExt {
    fn update_from_aixm(self, aixm: &[Member], tx: mpsc::Sender<Message>) -> Self;
}

pub(crate) enum EuroscopeFile {
    Sct {
        path: PathBuf,
        content: Box<Sct>,
    },
    #[expect(dead_code, reason = ".ese not handled yet")]
    Ese {
        path: PathBuf,
        content: Box<Ese>,
    },
    Isec {
        path: PathBuf,
        content: Box<IsecMap>,
    },
}
impl EuroscopeFile {
    pub(crate) fn combine_with_aixm(self, aixm: &[Member], tx: mpsc::Sender<Message>) -> Self {
        match self {
            EuroscopeFile::Sct { path, content } => {
                let content = Sct::update_from_aixm(*content, aixm, tx);
                EuroscopeFile::Sct {
                    path,
                    content: Box::new(content),
                }
            }
            EuroscopeFile::Isec { path, content } => {
                let content = IsecMap::update_from_aixm(*content, aixm, tx);
                EuroscopeFile::Isec {
                    path,
                    content: Box::new(content),
                }
            }
            EuroscopeFile::Ese {
                path: _,
                content: _,
            } => self,
        }
    }

    pub(crate) async fn write_file(self, tx: mpsc::Sender<Message>) -> AiracUpdaterResult {
        match self {
            Self::Sct {
                content: ref sct, ..
            } => {
                if let Some(file_name) = self.path().file_name() {
                    let mut bkp_file_name = file_name.to_os_string();
                    bkp_file_name.push(format!(".aau_bkp{}", Utc::now().format("%Y%m%d_%H%M%S")));
                    let bkp_file_path = self.path().with_file_name(bkp_file_name);
                    tx.send(Message::info(format!(
                        "Moving {} to {}",
                        self.path().display(),
                        bkp_file_path.display()
                    )))
                    .await?;

                    tokio::fs::rename(self.path(), &bkp_file_path)
                        .await
                        .context(RenameSnafu {
                            from: self.path().to_path_buf(),
                            to: bkp_file_path,
                        })?;

                    tx.send(Message::info(format!(
                        "Writing new {}",
                        self.path().display(),
                    )))
                    .await?;

                    OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(self.path())
                        .await
                        .context(CreateNewSnafu {
                            path: self.path().to_path_buf(),
                        })?
                        .write_all(sct.to_string().as_bytes())
                        .await
                        .context(WriteNewSnafu {
                            path: self.path().to_path_buf(),
                        })?;

                    tx.send(Message::info(format!(
                        "Finished writing {}",
                        self.path().display(),
                    )))
                    .await?;
                }
            }
            Self::Ese {
                path: _,
                content: _,
            } => (),
            Self::Isec {
                path: _,
                content: _,
            } => (),
        }
        Ok(())
    }

    fn path(&self) -> &Path {
        match self {
            EuroscopeFile::Sct { path, content: _ } => path,
            EuroscopeFile::Ese { path, content: _ } => path,
            EuroscopeFile::Isec { path, content: _ } => path,
        }
    }
}
