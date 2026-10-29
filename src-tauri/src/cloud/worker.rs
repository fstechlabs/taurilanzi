//! One sync: compares every document with the cloud, uploads what changed here, downloads what
//! changed elsewhere and hands it to the engine.

use std::collections::{BTreeMap, BTreeSet};

use jiff::Timestamp;
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;

use super::api::{Api, ApiError, Head, NewVersion, RemoteDocument};
use super::documents::{
    Kind, ProfileContent, SettingsContent, SyncChanges, local_docs, merge_settings,
};
use super::handle::Inner;
use super::plan::{self, Inputs, Step};
use super::state::Base;
use super::status::{JoinMode, JoinOffer};
use crate::config::{SCHEMA_VERSION, ScreensaverImage, assets_in};
use crate::engine::CommandError;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Engine(#[from] CommandError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// What a sync left to do soon.
pub struct Outcome {
    /// Something changed here that has to go up too, such as settings merged on joining.
    pub again: bool,
    /// The cloud has documents of a newer version of the app, which this one leaves alone.
    pub newer: bool,
}

struct Pull {
    id: Uuid,
    kind: Kind,
    head: Head,
}

impl Inner {
    pub(super) fn tick(&self, join: Option<JoinMode>) -> Result<Outcome, SyncError> {
        let api = self.api().ok_or(ApiError::Unauthorized)?;
        let remote = api.state()?;
        let mut join = join;
        if !self.lock().state.joined && join.is_none() {
            let cloud_profiles = remote
                .documents
                .iter()
                .filter(|d| d.kind == Kind::Profile && !d.head.deleted)
                .count();
            if cloud_profiles > 0 {
                self.lock().join = Some(JoinOffer {
                    cloud_profiles: cloud_profiles as u32,
                });
                return Ok(Outcome {
                    again: false,
                    newer: false,
                });
            }
            join = Some(JoinMode::Merge);
        }

        let export = self.engine.sync_export_blocking()?;
        let secrets = self.synced_secrets()?;
        let locals = local_docs(&export, secrets.clone())?;
        // Read after the engine answered: a profile deleted here only is marked before it goes.
        let (mut bases, ignored) = {
            let shared = self.lock();
            (shared.state.documents.clone(), shared.state.ignored.clone())
        };
        let remotes: BTreeMap<Uuid, &RemoteDocument> =
            remote.documents.iter().map(|d| (d.id, d)).collect();
        let ids: BTreeSet<Uuid> = locals
            .keys()
            .chain(remotes.keys())
            .chain(bases.keys())
            .copied()
            .collect();

        let mut changes = SyncChanges::default();
        let mut pulls = Vec::new();
        for id in ids {
            let local = locals.get(&id);
            let head = remotes.get(&id).map(|r| &r.head);
            let step = plan::decide(Inputs {
                local: local.map(|l| l.hash.as_str()),
                base: bases.get(&id),
                remote: head,
                local_only: export.local.contains(&id),
                ignored: ignored.contains_key(&id),
                joining: join,
            });
            match step {
                Step::Nothing => {}
                Step::Forget => {
                    bases.remove(&id);
                }
                Step::Remember => {
                    if let Some(head) = head {
                        bases.insert(id, remembered(head));
                    }
                }
                Step::DeleteLocal => {
                    changes.deleted.push(id);
                    match head {
                        Some(head) => {
                            changes.edited.insert(id, timestamp(&head.edited_at));
                            bases.insert(id, remembered(head));
                        }
                        None => {
                            bases.remove(&id);
                        }
                    }
                }
                Step::Pull => {
                    if let (Some(head), Some(document)) = (head, remotes.get(&id)) {
                        pulls.push(Pull {
                            id,
                            kind: document.kind,
                            head: head.clone(),
                        });
                    }
                }
                Step::Push { parent } => {
                    let Some(local) = local else { continue };
                    self.upload_assets(&api, &local.assets)?;
                    let version = Uuid::new_v4();
                    let edited_at = edit_time(local.edited_at, bases.get(&id));
                    let pushed = api.push(
                        id,
                        &NewVersion {
                            kind: local.kind,
                            version,
                            parent,
                            edited_at: edited_at.clone(),
                            deleted: false,
                            content: Some(&local.content),
                        },
                    )?;
                    if pushed.accepted {
                        bases.insert(
                            id,
                            Base {
                                version,
                                hash: Some(local.hash.clone()),
                                edited_at,
                            },
                        );
                    } else {
                        pulls.push(Pull {
                            id,
                            kind: local.kind,
                            head: pushed.head,
                        });
                    }
                }
                Step::PushDeletion { parent } => {
                    let version = Uuid::new_v4();
                    let edited_at = edit_time(export.edited.get(&id).copied(), bases.get(&id));
                    let pushed = api.push(
                        id,
                        &NewVersion {
                            kind: Kind::Profile,
                            version,
                            parent: Some(parent),
                            edited_at: edited_at.clone(),
                            deleted: true,
                            content: None,
                        },
                    )?;
                    if export.local.contains(&id) {
                        bases.remove(&id);
                    } else if pushed.accepted {
                        bases.insert(
                            id,
                            Base {
                                version,
                                hash: None,
                                edited_at,
                            },
                        );
                    } else {
                        pulls.push(Pull {
                            id,
                            kind: Kind::Profile,
                            head: pushed.head,
                        });
                    }
                }
            }
        }

        let mut newer = false;
        let mut merged = false;
        let mut pulled = Vec::new();
        for pull in pulls {
            let Pull { id, kind, head } = pull;
            if head.deleted {
                if locals.contains_key(&id) {
                    changes.deleted.push(id);
                }
                changes.edited.insert(id, timestamp(&head.edited_at));
                bases.insert(id, remembered(&head));
                continue;
            }
            let Some(content) = api.version(id, head.version)?.content else {
                continue;
            };
            match kind {
                Kind::Profile => {
                    let Some(content) = readable::<ProfileContent>(id, content) else {
                        newer = true;
                        continue;
                    };
                    if content.schema > SCHEMA_VERSION {
                        newer = true;
                        continue;
                    }
                    let assets: Vec<String> = assets_in(std::slice::from_ref(&content.profile))
                        .into_iter()
                        .map(str::to_owned)
                        .collect();
                    self.download_assets(&api, &assets)?;
                    changes.profiles.push(content.profile);
                }
                Kind::Settings => {
                    let Some(content) = readable::<SettingsContent>(id, content) else {
                        newer = true;
                        continue;
                    };
                    if content.schema > SCHEMA_VERSION {
                        newer = true;
                        continue;
                    }
                    if let ScreensaverImage::Custom { asset, source, .. } =
                        &content.shared.preferences.screensaver.image
                    {
                        let mut assets = vec![asset.clone()];
                        assets.extend(source.clone());
                        self.download_assets(&api, &assets)?;
                    }
                    let (shared, secrets) = if join == Some(JoinMode::Merge) {
                        merged = true;
                        merge_settings(content.shared, content.secrets, &export.settings, &secrets)
                    } else {
                        (content.shared, content.secrets)
                    };
                    changes.settings = Some(shared);
                    changes.secrets = Some(secrets);
                }
            }
            changes.edited.insert(id, timestamp(&head.edited_at));
            pulled.push((id, kind, head));
        }

        if !changes.is_empty() {
            let after = self.engine.apply_sync_blocking(changes)?;
            let docs = local_docs(&after, self.synced_secrets()?)?;
            for (id, kind, head) in pulled {
                // Merged settings have to go up as they are now.
                let hash = if merged && kind == Kind::Settings {
                    None
                } else {
                    docs.get(&id).map(|d| d.hash.clone())
                };
                bases.insert(
                    id,
                    Base {
                        version: head.version,
                        hash,
                        edited_at: head.edited_at,
                    },
                );
            }
        }

        let mut shared = self.lock();
        shared.state.documents = bases;
        shared
            .state
            .ignored
            .retain(|id, _| remotes.get(id).is_some_and(|r| !r.head.deleted));
        shared.state.joined = true;
        shared.state.last_sync = Some(format_time(Timestamp::now()));
        shared.join = None;
        self.file.save(&shared.state)?;
        Ok(Outcome {
            again: merged,
            newer,
        })
    }

    fn upload_assets(&self, api: &Api, ids: &[String]) -> Result<(), SyncError> {
        if ids.is_empty() {
            return Ok(());
        }
        for id in api.missing_assets(ids)? {
            match self.assets.bytes(&id) {
                Ok(bytes) => api.put_asset(&id, &bytes)?,
                Err(err) => log::warn!("an image of a synced profile is missing here: {err}"),
            }
        }
        Ok(())
    }

    fn download_assets(&self, api: &Api, ids: &[String]) -> Result<(), SyncError> {
        for id in ids {
            if self.assets.has(id) {
                continue;
            }
            match api.asset(id) {
                Ok(bytes) => self.assets.insert(id, &bytes)?,
                Err(ApiError::Status(404)) => {
                    log::warn!("an image of a synced profile is missing in the cloud")
                }
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    }
}

/// A document as this version reads it. None when a newer version put in it something this one
/// does not know, such as a new kind of action: it is left alone, as documents of a newer schema
/// are, instead of failing the whole sync.
fn readable<T: DeserializeOwned>(id: Uuid, content: Value) -> Option<T> {
    serde_json::from_value(content)
        .map_err(|err| log::warn!("document {id} is of a newer version of the app: {err}"))
        .ok()
}

fn remembered(head: &Head) -> Base {
    Base {
        version: head.version,
        hash: None,
        edited_at: head.edited_at.clone(),
    }
}

/// When the edit being uploaded was made: the engine's note if it is newer than the last sync,
/// or now, for edits it could not note, such as a password saved apart.
fn edit_time(noted: Option<Timestamp>, base: Option<&Base>) -> String {
    let since = base.and_then(|b| b.edited_at.parse::<Timestamp>().ok());
    let at = match (noted, since) {
        (Some(noted), Some(since)) if noted > since => noted,
        (Some(noted), None) => noted,
        _ => Timestamp::now(),
    };
    format_time(at)
}

fn timestamp(text: &str) -> Timestamp {
    text.parse().unwrap_or_else(|_| Timestamp::now())
}

/// With milliseconds, as JavaScript writes and reads them.
pub(super) fn format_time(at: Timestamp) -> String {
    at.strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(edited_at: &str) -> Base {
        Base {
            version: Uuid::nil(),
            hash: None,
            edited_at: edited_at.into(),
        }
    }

    #[test]
    fn documents_with_what_this_version_does_not_know_are_left_alone() {
        let profile = |action: serde_json::Value| {
            serde_json::json!({
                "schema": SCHEMA_VERSION,
                "profile": {
                    "id": Uuid::nil(), "name": "Casa", "home_page": Uuid::nil(), "apps": [],
                    "pages": [{
                        "id": Uuid::nil(), "name": "Inicio", "position": { "x": 0.0, "y": 0.0 },
                        "keys": { "0": {
                            "appearance": { "background": { "type": "solid", "color": { "r": 0, "g": 0, "b": 0 } }, "layers": [] },
                            "actions": [action],
                        } },
                    }],
                },
            })
        };
        let known = profile(serde_json::json!({ "type": "home" }));
        assert!(readable::<ProfileContent>(Uuid::nil(), known).is_some());
        let unknown = profile(serde_json::json!({ "type": "teleport" }));
        assert!(readable::<ProfileContent>(Uuid::nil(), unknown).is_none());
    }

    #[test]
    fn uploads_with_the_time_of_the_edit_when_it_is_known() {
        let noted: Timestamp = "2026-05-01T10:00:00.123456Z".parse().unwrap();
        assert_eq!(
            edit_time(Some(noted), Some(&base("2026-04-01T00:00:00.000Z"))),
            "2026-05-01T10:00:00.123Z"
        );
        assert_eq!(edit_time(Some(noted), None), "2026-05-01T10:00:00.123Z");
    }

    #[test]
    fn uploads_with_now_when_the_edit_is_older_than_the_last_sync_or_unknown() {
        let before = Timestamp::now();
        let noted: Timestamp = "2026-01-01T00:00:00Z".parse().unwrap();
        for at in [
            edit_time(Some(noted), Some(&base("2026-02-01T00:00:00.000Z"))),
            edit_time(None, Some(&base("2026-02-01T00:00:00.000Z"))),
            edit_time(None, None),
        ] {
            let at: Timestamp = at.parse().unwrap();
            // Written with milliseconds, so it may fall up to one before `before`.
            assert!(at + jiff::SignedDuration::from_millis(1) >= before);
        }
    }
}
