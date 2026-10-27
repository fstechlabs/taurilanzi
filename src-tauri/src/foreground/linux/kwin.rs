//! KDE on Wayland lets scripts loaded into KWin follow the focus. The script calls a D-Bus object
//! of the app's whenever another window takes it.

use std::sync::{Arc, Mutex, PoisonError};

use zbus::blocking::{Connection, Proxy, connection};

use super::apps;
use super::backend::Front;

const SCRIPT: &str = include_str!("kwin-focus.js");
const PATH: &str = "/com/taurilanzi/Focus";

type Latest = Arc<Mutex<Option<(String, String, u32)>>>;

pub struct Kwin {
    latest: Latest,
    /// Owns the name the script calls; dropping it would leave the script talking to nobody.
    _connection: Connection,
}

struct Focus {
    latest: Latest,
}

#[zbus::interface(name = "com.taurilanzi.Focus")]
impl Focus {
    fn activated(&self, desktop_file: String, class: String, pid: String) {
        let pid = pid.parse().unwrap_or(0);
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((desktop_file, class, pid));
    }
}

impl Kwin {
    pub fn start() -> Option<Self> {
        match Self::try_start() {
            Ok(kwin) => Some(kwin),
            Err(err) => {
                log::warn!("could not load the KWin script: {err}");
                None
            }
        }
    }

    fn try_start() -> zbus::Result<Self> {
        let latest: Latest = Arc::default();
        // Each copy of the app its own name, so a development build runs next to the released one.
        let pid = std::process::id();
        let name = format!("com.taurilanzi.Focus.p{pid}");
        let plugin = format!("taurilanzi-focus-{pid}");
        let connection = connection::Builder::session()?
            .name(name.as_str())?
            .serve_at(
                PATH,
                Focus {
                    latest: latest.clone(),
                },
            )?
            .build()?;
        let file = std::env::temp_dir().join(format!("{plugin}.js"));
        std::fs::write(&file, SCRIPT.replace("@NAME@", &name))
            .map_err(|err| zbus::Error::Failure(err.to_string()))?;
        let scripting = Proxy::new(
            &connection,
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
        )?;
        let _: zbus::Result<bool> = scripting.call("unloadScript", &(plugin.as_str(),));
        let id: i32 = scripting.call(
            "loadScript",
            &(file.to_string_lossy().as_ref(), plugin.as_str()),
        )?;
        // Plasma 6 puts each script under /Scripting, Plasma 5 at the root.
        let ran = [format!("/Scripting/Script{id}"), format!("/{id}")]
            .into_iter()
            .any(|path| {
                Proxy::new(&connection, "org.kde.KWin", path, "org.kde.kwin.Script")
                    .and_then(|script| script.call::<_, _, ()>("run", &()))
                    .is_ok()
            });
        if !ran {
            return Err(zbus::Error::Failure("the script did not start".into()));
        }
        Ok(Self {
            latest,
            _connection: connection,
        })
    }

    pub fn frontmost(&self) -> Option<Front> {
        let (desktop_file, class, pid) = self
            .latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()?;
        let app = apps::named(&desktop_file)
            .or_else(|| (pid != 0).then(|| apps::of_process(pid)).flatten())
            .or_else(|| apps::named(&class));
        Some(Front { pid, app })
    }
}
