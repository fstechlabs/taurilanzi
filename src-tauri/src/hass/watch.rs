//! What a key of two states follows in Home Assistant, showing its second state while it is on.

use super::domains::{is_on, option_spec};
use super::entities::domain;
use super::status::HassData;
use crate::config::{HassChoice, HassCommand, HassMedia};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Watch {
    /// The entity is on, in the words of its domain.
    Active(String),
    /// The entity is on this option; `attribute` is where it keeps it, or its state.
    Chosen {
        entity: String,
        attribute: Option<&'static str>,
        option: String,
    },
    Playing(String),
    Muted(String),
    Shuffling(String),
}

impl Watch {
    pub fn of(command: &HassCommand) -> Option<Self> {
        let active = |entity: &str| Some(Self::Active(entity.to_string()));
        match command {
            HassCommand::Switch { entity, .. }
            | HassCommand::Level { entity, .. }
            | HassCommand::Color { entity, .. }
            | HassCommand::Alarm { entity, .. }
            | HassCommand::Show { entity }
            | HassCommand::Tile { entity, .. } => active(entity),
            HassCommand::Run { entity } if matches!(domain(entity), "script" | "automation") => {
                active(entity)
            }
            HassCommand::Choose {
                entity,
                option,
                choice: HassChoice::Set { option: chosen },
            } => option_spec(*option, domain(entity)).map(|spec| Self::Chosen {
                entity: entity.clone(),
                attribute: spec.current,
                option: chosen.clone(),
            }),
            HassCommand::Media { entity, control } => match control {
                HassMedia::PlayPause | HassMedia::Play | HassMedia::Pause | HassMedia::Stop => {
                    Some(Self::Playing(entity.clone()))
                }
                HassMedia::Mute => Some(Self::Muted(entity.clone())),
                HassMedia::Shuffle => Some(Self::Shuffling(entity.clone())),
                _ => None,
            },
            HassCommand::Call { target, .. } => target.entities.first().and_then(|e| active(e)),
            _ => None,
        }
    }

    pub fn entity(&self) -> &str {
        match self {
            Self::Active(entity)
            | Self::Chosen { entity, .. }
            | Self::Playing(entity)
            | Self::Muted(entity)
            | Self::Shuffling(entity) => entity,
        }
    }

    /// Off while Home Assistant is not connected or the entity is unknown.
    pub fn is_on(&self, data: &HassData) -> bool {
        let Some(entity) = data.entity(self.entity()) else {
            return false;
        };
        match self {
            Self::Active(id) => is_on(id, entity),
            Self::Chosen {
                attribute, option, ..
            } => match attribute {
                Some(attribute) => entity.text(attribute) == Some(option.as_str()),
                None => entity.state == *option,
            },
            Self::Playing(_) => entity.state == "playing",
            Self::Muted(_) => entity.flag("is_volume_muted").unwrap_or(false),
            Self::Shuffling(_) => entity.flag("shuffle").unwrap_or(false),
        }
    }
}
