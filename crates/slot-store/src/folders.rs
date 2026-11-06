//! A folder's own settings, in `Config/folders.txt` as `GBA/Pokemon Hacks.sort = recent`.

use std::path::Path;

use crate::platform::Platform;

pub const FOLDERS_FILE: &str = "Config/folders.txt";

/// The order a folder's games stand in on its shelf.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum FolderSort {
    #[default]
    Az,
    Recent,
    Platform,
}

impl FolderSort {
    pub const ALL: [FolderSort; 3] = [FolderSort::Az, FolderSort::Recent, FolderSort::Platform];

    pub fn as_str(self) -> &'static str {
        match self {
            FolderSort::Az => "az",
            FolderSort::Recent => "recent",
            FolderSort::Platform => "platform",
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            FolderSort::Az => "A-Z",
            FolderSort::Recent => "Recent",
            FolderSort::Platform => "Platform",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// Which torn paper the folder's name is written on: the platform's own, or one of the three.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum FolderPaper {
    #[default]
    Auto,
    Light,
    Cuts,
    Deep,
}

impl FolderPaper {
    pub const ALL: [FolderPaper; 4] = [
        FolderPaper::Auto,
        FolderPaper::Light,
        FolderPaper::Cuts,
        FolderPaper::Deep,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            FolderPaper::Auto => "auto",
            FolderPaper::Light => "light",
            FolderPaper::Cuts => "cuts",
            FolderPaper::Deep => "deep",
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            FolderPaper::Auto => "Auto",
            FolderPaper::Light => "Light",
            FolderPaper::Cuts => "Cuts",
            FolderPaper::Deep => "Deep",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct FolderPrefs {
    /// The game shown in front of the stack; the first by name when unset.
    pub cover: Option<String>,
    pub sort: FolderSort,
    /// Open the folder on the game played last in it rather than its first.
    pub open_last: bool,
    pub last: Option<String>,
    pub paper: FolderPaper,
}

/// `GBA/Pokemon Hacks`: the folder's system and name.
pub fn folder_key(platform: Platform, name: &str) -> String {
    format!("{}/{name}", platform.dir_name())
}

pub fn read_folder_prefs(root: &Path, key: &str) -> FolderPrefs {
    let all = crate::ini::read(root, FOLDERS_FILE);
    let get = |field: &str| all.get(&format!("{key}.{field}")).cloned();
    FolderPrefs {
        cover: get("cover"),
        sort: get("sort")
            .as_deref()
            .and_then(FolderSort::parse)
            .unwrap_or_default(),
        open_last: get("open").as_deref() == Some("last"),
        last: get("last"),
        paper: get("paper")
            .as_deref()
            .and_then(FolderPaper::parse)
            .unwrap_or_default(),
    }
}

pub fn write_folder_prefs(root: &Path, key: &str, prefs: &FolderPrefs) -> std::io::Result<()> {
    let fields: [(&str, Option<&str>); 5] = [
        ("cover", prefs.cover.as_deref()),
        (
            "sort",
            (prefs.sort != FolderSort::Az).then_some(prefs.sort.as_str()),
        ),
        ("open", prefs.open_last.then_some("last")),
        ("last", prefs.last.as_deref()),
        (
            "paper",
            (prefs.paper != FolderPaper::Auto).then_some(prefs.paper.as_str()),
        ),
    ];
    for (field, value) in fields {
        crate::ini::put(root, FOLDERS_FILE, &format!("{key}.{field}"), value)?;
    }
    Ok(())
}

/// Moves a renamed folder's settings and its core to its new name.
pub fn rename_folder_prefs(
    root: &Path,
    from: &str,
    to: &str,
    platform: Platform,
) -> std::io::Result<()> {
    let prefs = read_folder_prefs(root, from);
    write_folder_prefs(root, from, &FolderPrefs::default())?;
    write_folder_prefs(root, to, &prefs)?;
    let core = crate::core::folder_core(root, from, platform);
    crate::core::write_folder_core(root, from, None)?;
    crate::core::write_folder_core(root, to, core)
}
