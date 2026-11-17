use std::str::FromStr;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CanvasOpening {
    #[default]
    Latest,
    Snapshot,
}

impl CanvasOpening {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Latest => "latest",
            Self::Snapshot => "snapshot",
        }
    }
}

impl FromStr for CanvasOpening {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "latest" => Ok(Self::Latest),
            "snapshot" => Ok(Self::Snapshot),
            other => Err(format!("{other} is not a way to open a canvas")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionKind {
    Snapshot,
    Autosave,
}

impl VersionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Snapshot => "snapshot",
            Self::Autosave => "autosave",
        }
    }
}

impl FromStr for VersionKind {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "snapshot" => Ok(Self::Snapshot),
            "autosave" => Ok(Self::Autosave),
            other => Err(format!("{other} is not a canvas version kind")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_survive_their_names() {
        for kind in [VersionKind::Snapshot, VersionKind::Autosave] {
            assert_eq!(kind.as_str().parse::<VersionKind>(), Ok(kind));
        }
        for opening in [CanvasOpening::Latest, CanvasOpening::Snapshot] {
            assert_eq!(opening.as_str().parse::<CanvasOpening>(), Ok(opening));
        }
    }
}
