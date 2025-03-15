use aixm::{AixmAirportHeliport, AixmDesignatedPoint, AixmNdb, AixmVor, LocationType, Member};
use geo::coord;
use snafu::{OptionExt as _, ResultExt as _};
use std::path::{Path, PathBuf};
use tokio::{fs::File, io::AsyncReadExt as _, sync::mpsc, task::JoinSet};
use tracing::{debug, error};
use vatsim_parser::{
    adaptation::locations::{Fix, VOR},
    ese::Ese,
    sct::{Airport, Sct},
};

use crate::{
    AiracUpdaterResult, Message, OpenEseSnafu, OpenSctSnafu, ParseEseSnafu, ParseSctSnafu,
    ProcessingSnafu, ReadDirSnafu, ReadEseSnafu, ReadSctSnafu,
};

pub(crate) enum EuroscopeFile {
    Sct { path: PathBuf, content: Box<Sct> },
    Ese { path: PathBuf, content: Box<Ese> },
}
impl EuroscopeFile {
    pub(crate) fn combine_with_aixm(self, aixm: &[Member]) -> Self {
        match self {
            EuroscopeFile::Sct { path, content } => {
                let content = Sct::update_from_aixm(*content, aixm);
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
}

trait SctAixmUpdateExt {
    fn update_from_aixm(self, aixm: &[Member]) -> Self;
}

fn update_airports(sct: &mut Sct, aixm_airport: &AixmAirportHeliport) {
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
        debug!("Adding new airport: {designator}");
        sct.airports.push(Airport {
            designator: designator.clone(),
            coordinate,
            ctr_airspace: "D".to_string(),
        });
    }
}

fn update_vors(sct: &mut Sct, aixm_vor: &AixmVor) {
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
        debug!(
            "Adding new VOR: {} {:.3}",
            aixm_vor.aixm_time_slice.aixm_vortime_slice.aixm_designator,
            aixm_vor
                .aixm_time_slice
                .aixm_vortime_slice
                .aixm_frequency
                .value
        );
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

fn update_ndbs(sct: &mut Sct, aixm_ndb: &AixmNdb) {
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
        debug!(
            "Adding new NDB: {} {:.3}",
            aixm_ndb.aixm_time_slice.aixm_ndbtime_slice.aixm_designator,
            aixm_ndb
                .aixm_time_slice
                .aixm_ndbtime_slice
                .aixm_frequency
                .value
        );
        sct.vors.push(VOR {
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

fn update_fixes(sct: &mut Sct, aixm_fix: &AixmDesignatedPoint) {
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
        debug!(
            "Adding new Fix: {}",
            aixm_fix
                .aixm_time_slice
                .aixm_designated_point_time_slice
                .aixm_designator,
        );
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
    fn update_from_aixm(mut self, aixm: &[Member]) -> Self {
        for data in aixm {
            match data {
                Member::AirportHeliport(aixm_airport_heliport) => {
                    update_airports(&mut self, aixm_airport_heliport);
                }
                Member::Vor(aixm_vor) => {
                    update_vors(&mut self, aixm_vor);
                }
                Member::Ndb(aixm_ndb) => {
                    update_ndbs(&mut self, aixm_ndb);
                }
                Member::DesignatedPoint(aixm_fix) => {
                    update_fixes(&mut self, aixm_fix);
                }
                _ => (),
            };
        }

        self
    }
}

pub(crate) async fn load_euroscope_files(
    dir: &Path,
    tx: mpsc::Sender<Message>,
) -> AiracUpdaterResult<Vec<EuroscopeFile>> {
    let mut join_handle = JoinSet::new();
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
