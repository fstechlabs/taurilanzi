//! Changing profiles, pages and keys: one key at a time, or many changes at once that apply
//! whole or not at all, and undo as one.

use std::collections::BTreeMap;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::context::{Answer, engine_error, json, refused, text};
use super::ids::{Names, parse_id};
use super::keys::prepare_key;
use crate::config::{AppRef, Config, Position};
use crate::engine::{Change, Gesture, KeySlot, PageChanges};
use crate::mcp::handler::Assistant;

/// Where the canvas puts a page to the right of another: a page card and the gap after it.
const PAGE_STEP: f32 = 400.0;

#[derive(Deserialize, JsonSchema, Clone, Copy)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// One change. Pages and profiles are given by id, or by the `name` (starting with `$`) that a
/// change earlier in the same call gave to what it created.
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ChangeInput {
    /// Puts a key in a slot, replacing what was there. `key` is a `Key` (see get_schema); fields
    /// it leaves out take their defaults, and a missing `appearance` takes the look of new keys.
    SetKey {
        page: String,
        index: u8,
        key: Value,
    },
    ClearKey {
        page: String,
        index: u8,
    },
    /// Moves a key within its profile; it swaps places with a key already there.
    MoveKey {
        from_page: String,
        from_index: u8,
        to_page: String,
        to_index: u8,
    },
    /// A new page comes with a back key in slot 0. Without `position` it goes to the right of the
    /// rightmost page of the canvas.
    CreatePage {
        profile: String,
        page_name: Option<String>,
        position: Option<Point>,
        name: Option<String>,
    },
    UpdatePage {
        page: String,
        page_name: Option<String>,
        position: Option<Point>,
    },
    DuplicatePage {
        page: String,
        name: Option<String>,
    },
    /// Keys that led to it stop doing so. The home page cannot be deleted.
    DeletePage {
        page: String,
    },
    SetHomePage {
        page: String,
    },
    /// A new profile has one empty page, its home; `home_name` names that page for later
    /// changes of the same call.
    CreateProfile {
        profile_name: Option<String>,
        name: Option<String>,
        home_name: Option<String>,
    },
    RenameProfile {
        profile: String,
        profile_name: String,
    },
    DuplicateProfile {
        profile: String,
        name: Option<String>,
    },
    /// Keys that switched to it stop doing so. The last profile cannot be deleted.
    DeleteProfile {
        profile: String,
    },
    /// Moves a profile to another place in the list, from 0.
    MoveProfile {
        profile: String,
        index: usize,
    },
    /// The profile comes to the dock when this app comes to the front (see list_running_apps).
    /// An app belongs to one profile at most.
    LinkApp {
        profile: String,
        app_id: String,
        app_name: String,
    },
    UnlinkApp {
        profile: String,
        app_id: String,
    },
}

#[derive(Deserialize, JsonSchema)]
pub struct ApplyParams {
    /// Applied in order; if one fails, none is.
    pub changes: Vec<ChangeInput>,
}

#[derive(Deserialize, JsonSchema)]
pub struct SetKeyParams {
    pub page: String,
    /// 0 to 12, or 13 for the wide key.
    pub index: u8,
    /// A `Key` (see get_schema), or null to empty the slot.
    pub key: Option<Value>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ProfileParams {
    pub profile: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct LocalParams {
    pub profile: String,
    /// True keeps it on this computer only, out of the cloud; false syncs it again.
    pub local: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GestureInput {
    Press,
    DoublePress,
    LongPress,
}

#[derive(Deserialize, JsonSchema)]
pub struct PressParams {
    pub page: String,
    pub index: u8,
    #[serde(default)]
    pub gesture: Option<GestureInput>,
}

#[tool_router(router = editing, vis = "pub(in crate::mcp)")]
impl Assistant {
    #[tool(
        description = "Applies a list of changes to profiles, pages and keys in order, all or none: if one fails nothing changes and the error says which one and why. The user undoes the whole call with one Undo. Name what you create (`name: \"$scenes\"`) to point to it from later changes of the same call, also inside keys (an open_page to \"$scenes\"). Answers the ids of what it created.",
        annotations(destructive_hint = true)
    )]
    async fn apply_changes(&self, Parameters(params): Parameters<ApplyParams>) -> Answer {
        let config = match self.tools.engine.config().await {
            Ok(config) => config,
            Err(err) => return engine_error(err),
        };
        let mut names = Names::default();
        let mut right_edges = right_edges(&config);
        let mut changes = Vec::with_capacity(params.changes.len());
        for (at, input) in params.changes.into_iter().enumerate() {
            match self.change(input, &config, &mut names, &mut right_edges) {
                Ok(change) => changes.push(change),
                Err(err) => return refused(format!("change {at}: {err}")),
            }
        }
        let changed = match self.tools.engine.apply(changes).await {
            Ok(changed) => changed,
            Err(err) => return engine_error(err),
        };
        // Nothing to undo after a lot that changed nothing, so the window offers nothing.
        if changed {
            self.tools.edited(true);
        }
        json(json!({ "applied": true, "changed": changed, "created": names.created() }))
    }

    #[tool(
        description = "Puts a key in a slot of a page, in any profile, or empties the slot with `key: null`. `key` is a `Key` (see get_schema and get_guide). Undoable.",
        annotations(destructive_hint = true)
    )]
    async fn set_key(&self, Parameters(params): Parameters<SetKeyParams>) -> Answer {
        let change = match params.key {
            Some(key) => ChangeInput::SetKey {
                page: params.page,
                index: params.index,
                key,
            },
            None => ChangeInput::ClearKey {
                page: params.page,
                index: params.index,
            },
        };
        self.apply_changes(Parameters(ApplyParams {
            changes: vec![change],
        }))
        .await
    }

    #[tool(description = "Shows a profile on the dock. It is not an edit: undo leaves it alone.")]
    async fn switch_profile(&self, Parameters(params): Parameters<ProfileParams>) -> Answer {
        let profile = match parse_id(&params.profile, "profile") {
            Ok(profile) => profile,
            Err(err) => return refused(err),
        };
        match self.tools.engine.set_profile(profile).await {
            Ok(()) => text("the dock shows that profile"),
            Err(err) => engine_error(err),
        }
    }

    #[tool(description = "Shows a page on the dock, switching to its profile first if needed.")]
    async fn show_page(
        &self,
        Parameters(params): Parameters<super::reading::PageParams>,
    ) -> Answer {
        let page = match parse_id(&params.page, "page") {
            Ok(page) => page,
            Err(err) => return refused(err),
        };
        let config = match self.tools.engine.config().await {
            Ok(config) => config,
            Err(err) => return engine_error(err),
        };
        let Some(owner) = config.page_owner(page) else {
            return refused(format!("page {page} does not exist"));
        };
        if owner.id != config.active_profile
            && let Err(err) = self.tools.engine.set_profile(owner.id).await
        {
            return engine_error(err);
        }
        match self.tools.engine.show_page(page).await {
            Ok(()) => text("the dock shows that page"),
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "Runs what a key does, as if it were pressed on the dock (or double pressed, or held), to try it. The key must be on a page of the profile on the dock. It really does it: opens apps, presses shortcuts, changes OBS…",
        annotations(destructive_hint = true)
    )]
    async fn press_key(&self, Parameters(params): Parameters<PressParams>) -> Answer {
        let page = match parse_id(&params.page, "page") {
            Ok(page) => page,
            Err(err) => return refused(err),
        };
        let gesture = match params.gesture.unwrap_or(GestureInput::Press) {
            GestureInput::Press => Gesture::Press,
            GestureInput::DoublePress => Gesture::DoublePress,
            GestureInput::LongPress => Gesture::LongPress,
        };
        match self
            .tools
            .engine
            .press_key(page, params.index, gesture)
            .await
        {
            Ok(()) => text("done"),
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "Keeps a profile on this computer only, out of the cloud, or syncs it again."
    )]
    async fn set_profile_local(&self, Parameters(params): Parameters<LocalParams>) -> Answer {
        let profile = match parse_id(&params.profile, "profile") {
            Ok(profile) => profile,
            Err(err) => return refused(err),
        };
        match self
            .tools
            .engine
            .set_profile_local(profile, params.local)
            .await
        {
            Ok(()) => {
                self.tools.edited(false);
                text("done")
            }
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "Undoes the last edit of profiles, pages or keys, the assistant's or the user's: a whole apply_changes at once.",
        annotations(destructive_hint = true)
    )]
    async fn undo(&self) -> Answer {
        match self.tools.engine.undo().await {
            Ok(()) => {
                self.tools.edited(false);
                text("undone")
            }
            Err(err) => engine_error(err),
        }
    }

    #[tool(description = "Redoes what undo took back.")]
    async fn redo(&self) -> Answer {
        match self.tools.engine.redo().await {
            Ok(()) => {
                self.tools.edited(false);
                text("redone")
            }
            Err(err) => engine_error(err),
        }
    }
}

impl Assistant {
    fn change(
        &self,
        input: ChangeInput,
        config: &Config,
        names: &mut Names,
        right_edges: &mut BTreeMap<Uuid, Option<Position>>,
    ) -> Result<Change, String> {
        Ok(match input {
            ChangeInput::SetKey { page, index, key } => {
                let page = names.resolve(&page, "page")?;
                let key = prepare_key(&self.tools, config, names, page, index, key)?;
                Change::SetKey {
                    page,
                    index,
                    key: Some(key),
                }
            }
            ChangeInput::ClearKey { page, index } => Change::SetKey {
                page: names.resolve(&page, "page")?,
                index,
                key: None,
            },
            ChangeInput::MoveKey {
                from_page,
                from_index,
                to_page,
                to_index,
            } => Change::MoveKey {
                from: KeySlot {
                    page: names.resolve(&from_page, "page")?,
                    index: from_index,
                },
                to: KeySlot {
                    page: names.resolve(&to_page, "page")?,
                    index: to_index,
                },
            },
            ChangeInput::CreatePage {
                profile,
                page_name,
                position,
                name,
            } => {
                let profile = names.resolve(&profile, "profile")?;
                let edge = right_edges.entry(profile).or_default();
                let position = position
                    .map(|p| Position { x: p.x, y: p.y })
                    .unwrap_or_else(|| {
                        edge.map_or(Position::default(), |right| Position {
                            x: right.x + PAGE_STEP,
                            y: right.y,
                        })
                    });
                if edge.is_none_or(|right| position.x > right.x) {
                    *edge = Some(position);
                }
                Change::CreatePage {
                    profile,
                    id: names.declare(name.as_deref())?,
                    name: page_name,
                    position,
                }
            }
            ChangeInput::UpdatePage {
                page,
                page_name,
                position,
            } => Change::UpdatePage {
                page: names.resolve(&page, "page")?,
                changes: PageChanges {
                    name: page_name,
                    position: position.map(|p| Position { x: p.x, y: p.y }),
                },
            },
            ChangeInput::DuplicatePage { page, name } => Change::DuplicatePage {
                page: names.resolve(&page, "page")?,
                id: names.declare(name.as_deref())?,
            },
            ChangeInput::DeletePage { page } => Change::DeletePage {
                page: names.resolve(&page, "page")?,
            },
            ChangeInput::SetHomePage { page } => Change::SetHomePage {
                page: names.resolve(&page, "page")?,
            },
            ChangeInput::CreateProfile {
                profile_name,
                name,
                home_name,
            } => {
                let id = names.declare(name.as_deref())?;
                let home = names.declare(home_name.as_deref())?;
                right_edges.insert(id, Some(Position::default()));
                Change::CreateProfile {
                    id,
                    home,
                    name: profile_name,
                }
            }
            ChangeInput::RenameProfile {
                profile,
                profile_name,
            } => Change::RenameProfile {
                profile: names.resolve(&profile, "profile")?,
                name: profile_name,
            },
            ChangeInput::DuplicateProfile { profile, name } => Change::DuplicateProfile {
                profile: names.resolve(&profile, "profile")?,
                id: names.declare(name.as_deref())?,
            },
            ChangeInput::DeleteProfile { profile } => Change::DeleteProfile {
                profile: names.resolve(&profile, "profile")?,
            },
            ChangeInput::MoveProfile { profile, index } => Change::MoveProfile {
                profile: names.resolve(&profile, "profile")?,
                index,
            },
            ChangeInput::LinkApp {
                profile,
                app_id,
                app_name,
            } => Change::LinkApp {
                profile: names.resolve(&profile, "profile")?,
                app: AppRef {
                    id: app_id,
                    name: app_name,
                },
            },
            ChangeInput::UnlinkApp { profile, app_id } => Change::UnlinkApp {
                profile: names.resolve(&profile, "profile")?,
                app: app_id,
            },
        })
    }
}

/// The rightmost page of each profile, where the next new page goes after.
fn right_edges(config: &Config) -> BTreeMap<Uuid, Option<Position>> {
    config
        .profiles
        .iter()
        .map(|profile| {
            let right = profile
                .pages
                .iter()
                .map(|page| page.position)
                .max_by(|a, b| a.x.total_cmp(&b.x));
            (profile.id, right)
        })
        .collect()
}
