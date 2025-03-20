use std::path::{Path, PathBuf};

use aixm::{AixmAirportHeliport, AixmDesignatedPoint, AixmNdb, AixmVor, LocationType, Member};
use chrono::Utc;
use geo::{Distance as _, Geodesic, coord};
use snafu::ResultExt as _;
use tokio::{fs::OpenOptions, io::AsyncWriteExt, sync::mpsc};
use tracing::error;
use vatsim_parser::{
    adaptation::locations::{Fix, NDB, VOR},
    ese::Ese,
    sct::{Airport, Sct},
};

use crate::{
    Message,
    error::{AiracUpdaterResult, CreateNewSnafu, RenameSnafu, WriteNewSnafu},
};

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
            EuroscopeFile::Ese {
                path: _,
                content: _,
            } => self,
        }
    }

    pub(crate) async fn write_file(self, tx: mpsc::Sender<Message>) -> AiracUpdaterResult {
        if let Self::Sct { content: sct, .. } = &self {
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
        Ok(())
    }

    fn path(&self) -> &Path {
        match self {
            EuroscopeFile::Sct { path, content: _ } => path,
            EuroscopeFile::Ese { path, content: _ } => path,
        }
    }
}

trait SctAixmUpdateExt {
    fn update_from_aixm(self, aixm: &[Member], tx: mpsc::Sender<Message>) -> Self;
}

fn update_airports(sct: &mut Sct, aixm_airport: &AixmAirportHeliport, tx: mpsc::Sender<Message>) {
    let (lat, lng) = aixm_airport
        .aixm_time_slice
        .aixm_airport_heliport_time_slice
        .aixm_arp
        .aixm_elevated_point
        .gml_pos
        .split_once(' ')
        .unwrap();
    let coordinate = coord! {
        x: lng.parse().unwrap(),
        y: lat.parse().unwrap(),
    };
    if let Some(ad) = sct.airports.iter_mut().find(|ad| {
        aixm_airport
            .aixm_time_slice
            .aixm_airport_heliport_time_slice
            .aixm_location_indicator_icao
            .text
            .as_ref()
            .is_some_and(|designator| *designator == ad.designator)
    }) {
        ad.coordinate = coordinate;
    } else if let Some(designator) = &aixm_airport
        .aixm_time_slice
        .aixm_airport_heliport_time_slice
        .aixm_location_indicator_icao
        .text
    {
        if let Err(e) =
            tx.blocking_send(Message::debug(format!("Adding new airport: {designator}")))
        {
            error!("{e}");
        }
        sct.airports.push(Airport {
            designator: designator.clone(),
            coordinate,
            ctr_airspace: "D".to_string(),
        });
    }
}

fn update_vors(sct: &mut Sct, aixm_vor: &AixmVor, tx: mpsc::Sender<Message>) {
    let (lat, lng) = (match &aixm_vor
        .aixm_time_slice
        .aixm_vortime_slice
        .aixm_location
        .location
    {
        LocationType::ElevatedPoint(ep) => &ep.gml_pos,
        LocationType::Point(p) => &p.gml_pos,
    })
    .split_once(' ')
    .unwrap();
    let coordinate = coord! {
        x: lng.parse().unwrap(),
        y: lat.parse().unwrap(),
    };
    if let Some(vor) = sct.vors.iter_mut().find(|vor| {
        aixm_vor.aixm_time_slice.aixm_vortime_slice.aixm_designator == vor.designator
            && format!(
                "{:.3}",
                aixm_vor
                    .aixm_time_slice
                    .aixm_vortime_slice
                    .aixm_frequency
                    .value
            ) == vor.frequency
    }) {
        vor.coordinate = coordinate;
    } else {
        if let Err(e) = tx.blocking_send(Message::debug(format!(
            "Adding new VOR: {} {:.3}",
            aixm_vor.aixm_time_slice.aixm_vortime_slice.aixm_designator,
            aixm_vor
                .aixm_time_slice
                .aixm_vortime_slice
                .aixm_frequency
                .value
        ))) {
            error!("{e}");
        }

        sct.vors.push(VOR {
            designator: aixm_vor
                .aixm_time_slice
                .aixm_vortime_slice
                .aixm_designator
                .clone(),
            coordinate,
            frequency: format!(
                "{:.3}",
                aixm_vor
                    .aixm_time_slice
                    .aixm_vortime_slice
                    .aixm_frequency
                    .value
            ),
        });
    }
}

fn update_ndbs(sct: &mut Sct, aixm_ndb: &AixmNdb, tx: mpsc::Sender<Message>) {
    let (lat, lng) = (match &aixm_ndb
        .aixm_time_slice
        .aixm_ndbtime_slice
        .aixm_location
        .location
    {
        LocationType::ElevatedPoint(ep) => &ep.gml_pos,
        LocationType::Point(p) => &p.gml_pos,
    })
    .split_once(' ')
    .unwrap();
    let coordinate = coord! {
        x: lng.parse().unwrap(),
        y: lat.parse().unwrap(),
    };
    if let Some(ndb) = sct.ndbs.iter_mut().find(|ndb| {
        aixm_ndb.aixm_time_slice.aixm_ndbtime_slice.aixm_designator == ndb.designator
            && format!(
                "{:.3}",
                aixm_ndb
                    .aixm_time_slice
                    .aixm_ndbtime_slice
                    .aixm_frequency
                    .value
            ) == ndb.frequency
    }) {
        ndb.coordinate = coordinate;
    } else {
        if let Err(e) = tx.blocking_send(Message::debug(format!(
            "Adding new NDB: {} {:.3}",
            aixm_ndb.aixm_time_slice.aixm_ndbtime_slice.aixm_designator,
            aixm_ndb
                .aixm_time_slice
                .aixm_ndbtime_slice
                .aixm_frequency
                .value
        ))) {
            error!("{e}");
        }
        sct.ndbs.push(NDB {
            designator: aixm_ndb
                .aixm_time_slice
                .aixm_ndbtime_slice
                .aixm_designator
                .clone(),
            coordinate,
            frequency: format!(
                "{:.3}",
                aixm_ndb
                    .aixm_time_slice
                    .aixm_ndbtime_slice
                    .aixm_frequency
                    .value
            ),
        });
    }
}

fn update_fixes(sct: &mut Sct, aixm_fix: &AixmDesignatedPoint, tx: mpsc::Sender<Message>) {
    let (lat, lng) = (match &aixm_fix
        .aixm_time_slice
        .aixm_designated_point_time_slice
        .aixm_location
        .location
    {
        LocationType::ElevatedPoint(ep) => &ep.gml_pos,
        LocationType::Point(p) => &p.gml_pos,
    })
    .split_once(' ')
    .unwrap();
    let coordinate = coord! {
        x: lng.parse().unwrap(),
        y: lat.parse().unwrap(),
    };
    if let Some(fix) = sct.fixes.iter_mut().find(|fix| {
        aixm_fix
            .aixm_time_slice
            .aixm_designated_point_time_slice
            .aixm_designator
            == fix.designator
        // FIXME check "near" to existing and add only if not exists, no modifying
    }) {
        fix.coordinate = coordinate;
    } else if aixm_fix
        .aixm_time_slice
        .aixm_designated_point_time_slice
        .aixm_designator
        .len()
        == 5
        && aixm_fix
            .aixm_time_slice
            .aixm_designated_point_time_slice
            .aixm_designator
            .chars()
            .next()
            .is_some_and(|c| !c.is_ascii_digit())
    {
        if let Err(e) = tx.blocking_send(Message::debug(format!(
            "Adding new Fix: {}",
            aixm_fix
                .aixm_time_slice
                .aixm_designated_point_time_slice
                .aixm_designator,
        ))) {
            error!("{e}");
        }
        sct.fixes.push(Fix {
            designator: aixm_fix
                .aixm_time_slice
                .aixm_designated_point_time_slice
                .aixm_designator
                .clone(),
            coordinate,
        });
    }
}

impl SctAixmUpdateExt for Sct {
    fn update_from_aixm(mut self, aixm: &[Member], tx: mpsc::Sender<Message>) -> Self {
        for data in aixm {
            match data {
                Member::AirportHeliport(aixm_airport_heliport) => {
                    update_airports(&mut self, aixm_airport_heliport, tx.clone());
                }
                Member::Vor(aixm_vor) => {
                    update_vors(&mut self, aixm_vor, tx.clone());
                }
                Member::Ndb(aixm_ndb) => {
                    update_ndbs(&mut self, aixm_ndb, tx.clone());
                }
                Member::DesignatedPoint(aixm_fix) => {
                    update_fixes(&mut self, aixm_fix, tx.clone());
                }
                _ => (),
            };
        }

        self
    }
}
