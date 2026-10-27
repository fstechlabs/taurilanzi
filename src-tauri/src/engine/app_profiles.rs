//! Which profile the dock shows as apps come to the front. Only the decisions, so they are easy
//! to follow; the engine does the switching. Runtime state, like the path: nothing here is saved
//! or undone.

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppProfiles {
    /// The profile the user picked last, to come back to when the app in front has none.
    manual: Option<Uuid>,
    /// The profile picked for the app in front, while the dock shows it.
    automatic: Option<Uuid>,
}

impl AppProfiles {
    pub fn new(active: Uuid) -> Self {
        Self {
            manual: Some(active),
            automatic: None,
        }
    }

    /// An app came to the front, linked to `linked` or to no profile. Returns the profile to
    /// switch to, if the dock has to change.
    pub fn front_changed(&mut self, linked: Option<Uuid>, active: Uuid) -> Option<Uuid> {
        let was_automatic = self.is_automatic(active);
        match linked {
            Some(profile) => {
                if !was_automatic {
                    self.manual = Some(active);
                }
                self.automatic = Some(profile);
                (profile != active).then_some(profile)
            }
            None => {
                self.automatic = None;
                if was_automatic {
                    self.manual.filter(|manual| *manual != active)
                } else {
                    None
                }
            }
        }
    }

    /// The user picked a profile, from the app, the menu bar or a key: it stays until an app with
    /// a profile comes to the front, and it is where the dock comes back to afterwards.
    pub fn chosen(&mut self, profile: Uuid) {
        self.manual = Some(profile);
        self.automatic = None;
    }

    /// Whether `active` is on the dock because of the app in front.
    pub fn is_automatic(&self, active: Uuid) -> bool {
        self.automatic == Some(active)
    }

    pub fn manual(&self) -> Option<Uuid> {
        self.manual
    }

    /// Forgets profiles that were deleted or undone.
    pub fn retain(&mut self, exists: impl Fn(Uuid) -> bool) {
        self.manual = self.manual.filter(|id| exists(*id));
        self.automatic = self.automatic.filter(|id| exists(*id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (Uuid, Uuid, Uuid) {
        (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
    }

    #[test]
    fn a_linked_app_switches_and_remembers_the_manual_profile() {
        let (manual, linked, _) = ids();
        let mut profiles = AppProfiles::new(manual);
        assert_eq!(profiles.front_changed(Some(linked), manual), Some(linked));
        assert!(profiles.is_automatic(linked));
        assert_eq!(profiles.manual(), Some(manual));
        assert_eq!(profiles.front_changed(None, linked), Some(manual));
        assert!(!profiles.is_automatic(manual));
    }

    #[test]
    fn going_from_one_linked_app_to_another_keeps_the_manual_profile() {
        let (manual, first, second) = ids();
        let mut profiles = AppProfiles::new(manual);
        profiles.front_changed(Some(first), manual);
        assert_eq!(profiles.front_changed(Some(second), first), Some(second));
        assert_eq!(profiles.manual(), Some(manual));
        assert_eq!(profiles.front_changed(None, second), Some(manual));
    }

    #[test]
    fn an_app_linked_to_the_profile_on_the_dock_changes_nothing() {
        let (manual, _, _) = ids();
        let mut profiles = AppProfiles::new(manual);
        assert_eq!(profiles.front_changed(Some(manual), manual), None);
        assert!(profiles.is_automatic(manual));
    }

    #[test]
    fn an_unlinked_app_only_goes_back_from_an_automatic_profile() {
        let (manual, other, _) = ids();
        let mut profiles = AppProfiles::new(manual);
        assert_eq!(profiles.front_changed(None, manual), None);
        profiles.chosen(other);
        assert_eq!(profiles.front_changed(None, other), None);
        assert_eq!(profiles.manual(), Some(other));
    }

    #[test]
    fn choosing_a_profile_makes_it_the_manual_one() {
        let (manual, linked, chosen) = ids();
        let mut profiles = AppProfiles::new(manual);
        profiles.front_changed(Some(linked), manual);
        profiles.chosen(chosen);
        assert!(!profiles.is_automatic(linked));
        assert_eq!(profiles.manual(), Some(chosen));
        assert_eq!(profiles.front_changed(None, chosen), None);
        assert_eq!(profiles.front_changed(Some(linked), chosen), Some(linked));
        assert_eq!(profiles.front_changed(None, linked), Some(chosen));
    }

    #[test]
    fn retain_forgets_deleted_profiles() {
        let (manual, linked, _) = ids();
        let mut profiles = AppProfiles::new(manual);
        profiles.front_changed(Some(linked), manual);
        profiles.retain(|id| id != linked);
        assert!(!profiles.is_automatic(linked));
        assert_eq!(profiles.manual(), Some(manual));
        profiles.retain(|_| false);
        assert_eq!(profiles.manual(), None);
        assert_eq!(profiles.front_changed(None, linked), None);
    }
}
