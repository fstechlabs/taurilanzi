//! What to do with each document, from how it is here, how it was at the last sync and how the
//! cloud has it now.

use uuid::Uuid;

use super::api::Head;
use super::state::Base;
use super::status::JoinMode;

#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    /// The hash of the document here; none when it is not here.
    pub local: Option<&'a str>,
    pub base: Option<&'a Base>,
    pub remote: Option<&'a Head>,
    /// A profile kept on this computer only.
    pub local_only: bool,
    /// A profile of the cloud left out of this computer.
    pub ignored: bool,
    /// The first sync of this computer with the account.
    pub joining: Option<JoinMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Nothing,
    /// Uploads this computer's version; the cloud keeps the most recent one as current.
    Push {
        parent: Option<Uuid>,
    },
    /// Uploads that it was deleted here.
    PushDeletion {
        parent: Uuid,
    },
    Pull,
    DeleteLocal,
    /// Notes the version of the cloud, which needs nothing here.
    Remember,
    /// Stops following the document.
    Forget,
}

pub fn decide(i: Inputs) -> Step {
    if i.local_only {
        // Made local here and taken out of the cloud too.
        return match (i.base, i.remote) {
            (Some(_), Some(remote)) if !remote.deleted && !i.ignored => Step::PushDeletion {
                parent: remote.version,
            },
            _ => Step::Forget,
        };
    }
    if i.ignored {
        return Step::Forget;
    }
    let Some(base) = i.base else {
        return first_time(i);
    };
    let Some(remote) = i.remote else {
        // The cloud lost it: what is here goes up again.
        return match i.local {
            Some(_) => Step::Push { parent: None },
            None => Step::Forget,
        };
    };
    let changed_here = i.local != base.hash.as_deref();
    let changed_there = remote.version != base.version;
    match (changed_here, changed_there) {
        (false, false) => Step::Nothing,
        (false, true) if remote.deleted => match i.local {
            Some(_) => Step::DeleteLocal,
            None => Step::Remember,
        },
        (false, true) => Step::Pull,
        (true, _) => match (i.local, remote.deleted && changed_there) {
            (None, true) => Step::Remember,
            (None, false) => Step::PushDeletion {
                parent: base.version,
            },
            (Some(_), _) => Step::Push {
                parent: Some(base.version),
            },
        },
    }
}

/// A document this computer never synced: new here, new there, or both when joining.
fn first_time(i: Inputs) -> Step {
    match (i.local, i.remote) {
        (Some(_), Some(remote)) => match i.joining {
            Some(JoinMode::Replace) if remote.deleted => Step::DeleteLocal,
            Some(_) if !remote.deleted => Step::Pull,
            // Synced again on purpose, or kept on joining: this computer's version wins.
            _ => Step::Push {
                parent: Some(remote.version),
            },
        },
        (Some(_), None) => match i.joining {
            Some(JoinMode::Replace) => Step::DeleteLocal,
            _ => Step::Push { parent: None },
        },
        (None, Some(remote)) if remote.deleted => Step::Remember,
        (None, Some(_)) => Step::Pull,
        (None, None) => Step::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(version: u128, hash: Option<&str>) -> Base {
        Base {
            version: Uuid::from_u128(version),
            hash: hash.map(str::to_owned),
            edited_at: "2026-01-01T00:00:00.000Z".into(),
        }
    }

    fn head(version: u128, deleted: bool) -> Head {
        Head {
            version: Uuid::from_u128(version),
            edited_at: "2026-01-01T00:00:00.000Z".into(),
            deleted,
        }
    }

    fn decide_with(
        local: Option<&str>,
        base: Option<&Base>,
        remote: Option<&Head>,
        tweak: impl FnOnce(&mut Inputs),
    ) -> Step {
        let mut inputs = Inputs {
            local,
            base,
            remote,
            local_only: false,
            ignored: false,
            joining: None,
        };
        tweak(&mut inputs);
        decide(inputs)
    }

    fn plain(local: Option<&str>, base: Option<&Base>, remote: Option<&Head>) -> Step {
        decide_with(local, base, remote, |_| {})
    }

    #[test]
    fn nothing_changed_needs_nothing() {
        assert_eq!(
            plain(Some("a"), Some(&base(1, Some("a"))), Some(&head(1, false))),
            Step::Nothing
        );
    }

    #[test]
    fn a_change_here_goes_up_on_top_of_the_last_sync() {
        assert_eq!(
            plain(Some("b"), Some(&base(1, Some("a"))), Some(&head(1, false))),
            Step::Push {
                parent: Some(Uuid::from_u128(1))
            }
        );
    }

    #[test]
    fn a_change_elsewhere_comes_down() {
        assert_eq!(
            plain(Some("a"), Some(&base(1, Some("a"))), Some(&head(2, false))),
            Step::Pull
        );
    }

    #[test]
    fn changes_on_both_sides_go_up_for_the_cloud_to_keep_the_most_recent() {
        assert_eq!(
            plain(Some("b"), Some(&base(1, Some("a"))), Some(&head(2, false))),
            Step::Push {
                parent: Some(Uuid::from_u128(1))
            }
        );
    }

    #[test]
    fn deleted_elsewhere_is_deleted_here() {
        assert_eq!(
            plain(Some("a"), Some(&base(1, Some("a"))), Some(&head(2, true))),
            Step::DeleteLocal
        );
        assert_eq!(
            plain(None, Some(&base(1, None)), Some(&head(2, true))),
            Step::Remember
        );
    }

    #[test]
    fn deleted_here_goes_up_as_a_deletion() {
        assert_eq!(
            plain(None, Some(&base(1, Some("a"))), Some(&head(1, false))),
            Step::PushDeletion {
                parent: Uuid::from_u128(1)
            }
        );
        // Edited elsewhere meanwhile: the cloud keeps the most recent of both.
        assert_eq!(
            plain(None, Some(&base(1, Some("a"))), Some(&head(2, false))),
            Step::PushDeletion {
                parent: Uuid::from_u128(1)
            }
        );
        assert_eq!(
            plain(None, Some(&base(1, Some("a"))), Some(&head(2, true))),
            Step::Remember
        );
    }

    #[test]
    fn brought_back_here_after_a_deletion_goes_up_again() {
        assert_eq!(
            plain(Some("a"), Some(&base(1, None)), Some(&head(1, true))),
            Step::Push {
                parent: Some(Uuid::from_u128(1))
            }
        );
    }

    #[test]
    fn what_the_cloud_lost_goes_up_again() {
        assert_eq!(
            plain(Some("a"), Some(&base(1, Some("a"))), None),
            Step::Push { parent: None }
        );
        assert_eq!(plain(None, Some(&base(1, None)), None), Step::Forget);
    }

    #[test]
    fn new_here_goes_up_and_new_elsewhere_comes_down() {
        assert_eq!(plain(Some("a"), None, None), Step::Push { parent: None });
        assert_eq!(plain(None, None, Some(&head(1, false))), Step::Pull);
        assert_eq!(plain(None, None, Some(&head(1, true))), Step::Remember);
        assert_eq!(plain(None, None, None), Step::Nothing);
    }

    #[test]
    fn synced_again_on_purpose_wins_over_the_cloud() {
        assert_eq!(
            plain(Some("a"), None, Some(&head(1, false))),
            Step::Push {
                parent: Some(Uuid::from_u128(1))
            }
        );
    }

    #[test]
    fn profiles_kept_here_only_stay_out() {
        let base = base(1, Some("a"));
        let remote = head(1, false);
        assert_eq!(
            decide_with(Some("a"), Some(&base), Some(&remote), |i| i.local_only =
                true),
            Step::PushDeletion {
                parent: Uuid::from_u128(1)
            },
            "made local and taken out of the cloud"
        );
        assert_eq!(
            decide_with(Some("a"), Some(&base), Some(&remote), |i| {
                i.local_only = true;
                i.ignored = true;
            }),
            Step::Forget,
            "made local while the others keep their copy"
        );
        assert_eq!(
            decide_with(Some("a"), None, Some(&head(2, true)), |i| i.local_only =
                true),
            Step::Forget
        );
    }

    #[test]
    fn profiles_deleted_here_only_stay_out() {
        assert_eq!(
            decide_with(
                None,
                Some(&base(1, Some("a"))),
                Some(&head(2, false)),
                |i| { i.ignored = true }
            ),
            Step::Forget
        );
    }

    #[test]
    fn joining_keeps_both_or_only_the_clouds() {
        let remote = head(1, false);
        let merge = |i: &mut Inputs| i.joining = Some(JoinMode::Merge);
        let replace = |i: &mut Inputs| i.joining = Some(JoinMode::Replace);
        assert_eq!(
            decide_with(Some("a"), None, Some(&remote), merge),
            Step::Pull
        );
        assert_eq!(
            decide_with(Some("a"), None, Some(&remote), replace),
            Step::Pull
        );
        assert_eq!(
            decide_with(Some("a"), None, None, merge),
            Step::Push { parent: None }
        );
        assert_eq!(
            decide_with(Some("a"), None, None, replace),
            Step::DeleteLocal
        );
        assert_eq!(
            decide_with(Some("a"), None, Some(&head(1, true)), replace),
            Step::DeleteLocal
        );
        assert_eq!(
            decide_with(Some("a"), None, Some(&head(1, true)), merge),
            Step::Push {
                parent: Some(Uuid::from_u128(1))
            }
        );
    }
}
