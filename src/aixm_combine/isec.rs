use aixm::{AixmDesignatedPoint, LocationType, Member};
use geo::{Distance, Geodesic, point};
use tokio::sync::mpsc;
use tracing::error;
use vatsim_parser::{adaptation::locations::Fix, isec::IsecMap};

use crate::Message;

use super::AixmUpdateExt;

impl AixmUpdateExt for IsecMap {
    fn update_from_aixm(mut self, aixm: &[Member], tx: mpsc::Sender<Message>) -> Self {
        for data in aixm {
            if let Member::DesignatedPoint(aixm_fix) = data {
                update_fixes(&mut self, aixm_fix, tx.clone());
            }
        }

        self
    }
}

fn update_fixes(isecs: &mut IsecMap, aixm_fix: &AixmDesignatedPoint, tx: mpsc::Sender<Message>) {
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
    let coordinate = point! {
        x: lng.parse().unwrap(),
        y: lat.parse().unwrap(),
    };
    if let Some(fix) = isecs
        .get_vec_mut(
            &aixm_fix
                .aixm_time_slice
                .aixm_designated_point_time_slice
                .aixm_designator,
        )
        .and_then(|fixes_with_name| {
            fixes_with_name.iter_mut().find(|fix| {
                aixm_fix
                    .aixm_time_slice
                    .aixm_designated_point_time_slice
                    .aixm_designator
                    == fix.designator
                    && Geodesic.distance(coordinate, fix.coordinate) < 1000.0
            })
        })
    {
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
        isecs.insert(
            aixm_fix
                .aixm_time_slice
                .aixm_designated_point_time_slice
                .aixm_designator
                .clone(),
            Fix {
                designator: aixm_fix
                    .aixm_time_slice
                    .aixm_designated_point_time_slice
                    .aixm_designator
                    .clone(),
                coordinate,
            },
        );
    }
}
