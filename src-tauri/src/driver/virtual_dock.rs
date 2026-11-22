//! Docks that exist only in the app, of any model: for working on the app without the devices,
//! and for the end-to-end tests. They take every request and show nothing.

use std::sync::mpsc::Sender;
use std::thread;

use log::info;

use super::catalog;
use super::device::DeviceInfo;
use super::dock::{Dock, DockEvent, Event, Reporter, next_connection};
use super::model::DeviceModel;

/// The models in a list of model ids separated by commas, as the environment gives them. Unknown
/// ids are left out.
pub fn parse(list: &str) -> Vec<&'static DeviceModel> {
    list.split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .filter_map(|id| {
            let model = catalog::model(id);
            if model.is_none() {
                log::warn!("no dock model {id}");
            }
            model
        })
        .collect()
}

/// Connects one virtual dock of each model listed; the n-th has serial `VIRTUAL-<n>`.
pub fn start(models: &[&'static DeviceModel], events: &Sender<DockEvent>) {
    for (n, model) in models.iter().enumerate() {
        let reporter = Reporter::new(next_connection(), events.clone());
        let (dock, requests) = Dock::channel();
        let info = DeviceInfo {
            serial: format!("VIRTUAL-{}", n + 1),
            model: model.name.into(),
            firmware: "virtual".into(),
            hardware: "virtual".into(),
        };
        let attached = Event::Attached {
            model,
            dock,
            simulated: true,
        };
        if !reporter.send(attached) || !reporter.send(Event::Connected(info)) {
            return;
        }
        info!("virtual {} connected", model.name);
        let _ = thread::Builder::new()
            .name(format!("virtual-dock-{}", reporter.connection))
            .spawn(move || for _ in requests {});
    }
}

/// What a dock says as it is plugged in, for the engine's tests: found, then who it is. Its
/// requests go nowhere.
#[cfg(test)]
pub fn plugged(model: &'static DeviceModel, serial: &str, simulated: bool) -> [DockEvent; 2] {
    let connection = next_connection();
    let (dock, _) = Dock::channel();
    let info = DeviceInfo {
        serial: serial.into(),
        model: model.name.into(),
        firmware: "test".into(),
        hardware: "test".into(),
    };
    [
        DockEvent {
            connection,
            event: Event::Attached {
                model,
                dock,
                simulated,
            },
        },
        DockEvent {
            connection,
            event: Event::Connected(info),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_models_and_skips_unknown_ones() {
        let models = parse(" ulanzi.d200, nobody.x ,,ulanzi.d200");
        assert_eq!(
            models.iter().map(|m| m.id).collect::<Vec<_>>(),
            ["ulanzi.d200", "ulanzi.d200"]
        );
    }
}
