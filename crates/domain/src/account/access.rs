use std::{fmt, str::FromStr};

use crate::account::{TeamId, UserId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    User(UserId),
    Team(TeamId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TeamRole {
    Viewer,
    Editor,
    Admin,
    Owner,
}

impl TeamRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Editor => "editor",
            Self::Admin => "admin",
            Self::Owner => "owner",
        }
    }

    pub fn can_edit(self) -> bool {
        self >= Self::Editor
    }

    pub fn can_manage(self) -> bool {
        self >= Self::Admin
    }

    pub fn can_grant(self, role: TeamRole) -> bool {
        match self {
            Self::Owner => true,
            Self::Admin => role < Self::Owner,
            _ => false,
        }
    }
}

impl fmt::Display for TeamRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TeamRole {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "viewer" => Ok(Self::Viewer),
            "editor" => Ok(Self::Editor),
            "admin" => Ok(Self::Admin),
            "owner" => Ok(Self::Owner),
            other => Err(format!("{other} is not a team role")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShareAccess {
    View,
    Edit,
}

impl ShareAccess {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Edit => "edit",
        }
    }
}

impl FromStr for ShareAccess {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "view" => Ok(Self::View),
            "edit" => Ok(Self::Edit),
            other => Err(format!("{other} is not a share access")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CanvasAccess {
    View,
    Edit,
    Manage,
}

impl CanvasAccess {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Edit => "edit",
            Self::Manage => "manage",
        }
    }

    pub fn can_edit(self) -> bool {
        self >= Self::Edit
    }

    pub fn can_manage(self) -> bool {
        self == Self::Manage
    }
}

impl From<ShareAccess> for CanvasAccess {
    fn from(share: ShareAccess) -> Self {
        match share {
            ShareAccess::View => Self::View,
            ShareAccess::Edit => Self::Edit,
        }
    }
}

pub fn canvas_access(
    viewer: UserId,
    owner: Owner,
    team_role: Option<TeamRole>,
    share: Option<ShareAccess>,
) -> Option<CanvasAccess> {
    let owned = match owner {
        Owner::User(user) if user == viewer => Some(CanvasAccess::Manage),
        Owner::User(_) => None,
        Owner::Team(_) => team_role.map(|role| match role {
            TeamRole::Owner | TeamRole::Admin => CanvasAccess::Manage,
            TeamRole::Editor => CanvasAccess::Edit,
            TeamRole::Viewer => CanvasAccess::View,
        }),
    };

    owned.max(share.map(CanvasAccess::from))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LabelAccess {
    Read,
    Write,
}

pub fn label_access(
    viewer: UserId,
    owner: Owner,
    team_role: Option<TeamRole>,
) -> Option<LabelAccess> {
    match owner {
        Owner::User(user) if user == viewer => Some(LabelAccess::Write),
        Owner::User(_) => None,
        Owner::Team(_) => team_role.map(|role| {
            if role.can_edit() {
                LabelAccess::Write
            } else {
                LabelAccess::Read
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team() -> Owner {
        Owner::Team(TeamId::new())
    }

    #[test]
    fn a_personal_canvas_belongs_to_its_owner_alone() {
        let me = UserId::new();
        let stranger = UserId::new();

        assert_eq!(
            canvas_access(me, Owner::User(me), None, None),
            Some(CanvasAccess::Manage)
        );
        assert_eq!(canvas_access(stranger, Owner::User(me), None, None), None);
    }

    #[test]
    fn a_share_opens_a_personal_canvas() {
        let me = UserId::new();
        let friend = UserId::new();

        assert_eq!(
            canvas_access(friend, Owner::User(me), None, Some(ShareAccess::View)),
            Some(CanvasAccess::View)
        );
        assert_eq!(
            canvas_access(friend, Owner::User(me), None, Some(ShareAccess::Edit)),
            Some(CanvasAccess::Edit)
        );
    }

    #[test]
    fn a_team_canvas_follows_the_team_role() {
        let me = UserId::new();

        assert_eq!(
            canvas_access(me, team(), Some(TeamRole::Viewer), None),
            Some(CanvasAccess::View)
        );
        assert_eq!(
            canvas_access(me, team(), Some(TeamRole::Editor), None),
            Some(CanvasAccess::Edit)
        );
        assert_eq!(
            canvas_access(me, team(), Some(TeamRole::Admin), None),
            Some(CanvasAccess::Manage)
        );
        assert_eq!(
            canvas_access(me, team(), Some(TeamRole::Owner), None),
            Some(CanvasAccess::Manage)
        );
        assert_eq!(canvas_access(me, team(), None, None), None);
    }

    #[test]
    fn the_wider_of_role_and_share_wins() {
        let me = UserId::new();

        assert_eq!(
            canvas_access(me, team(), Some(TeamRole::Viewer), Some(ShareAccess::Edit)),
            Some(CanvasAccess::Edit)
        );
        assert_eq!(
            canvas_access(me, team(), Some(TeamRole::Admin), Some(ShareAccess::View)),
            Some(CanvasAccess::Manage)
        );
    }

    #[test]
    fn team_labels_are_written_by_editors_and_read_by_viewers() {
        let me = UserId::new();

        assert_eq!(
            label_access(me, team(), Some(TeamRole::Viewer)),
            Some(LabelAccess::Read)
        );
        assert_eq!(
            label_access(me, team(), Some(TeamRole::Editor)),
            Some(LabelAccess::Write)
        );
        assert_eq!(label_access(me, team(), None), None);
        assert_eq!(
            label_access(me, Owner::User(me), None),
            Some(LabelAccess::Write)
        );
        assert_eq!(label_access(me, Owner::User(UserId::new()), None), None);
    }

    #[test]
    fn only_an_owner_hands_out_ownership() {
        assert!(TeamRole::Owner.can_grant(TeamRole::Owner));
        assert!(TeamRole::Admin.can_grant(TeamRole::Admin));
        assert!(!TeamRole::Admin.can_grant(TeamRole::Owner));
        assert!(!TeamRole::Editor.can_grant(TeamRole::Viewer));
    }
}
