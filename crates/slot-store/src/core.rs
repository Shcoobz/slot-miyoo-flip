use std::collections::HashMap;
use std::path::Path;

use crate::Platform;

pub const SELECTED_CORE_FILE: &str = "Config/selected_core.txt";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Core {
    #[default]
    Mgba,
    Gpsp,
    Gambatte,
    Fceumm,
    Nestopia,
    Snes9x,
    /// Snes9x 2005 Plus: lighter than Snes9x, for battery and fast-forward.
    Snes9x2005,
}

impl Core {
    pub const ALL: [Core; 7] = [
        Core::Mgba,
        Core::Gpsp,
        Core::Gambatte,
        Core::Fceumm,
        Core::Nestopia,
        Core::Snes9x,
        Core::Snes9x2005,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Core::Mgba => "mgba",
            Core::Gpsp => "gpsp",
            Core::Gambatte => "gambatte",
            Core::Fceumm => "fceumm",
            Core::Nestopia => "nestopia",
            Core::Snes9x => "snes9x",
            Core::Snes9x2005 => "snes9x2005_plus",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn text(self) -> &'static str {
        match self {
            Core::Mgba => "mGBA",
            Core::Gpsp => "gpSP",
            Core::Gambatte => "Gambatte",
            Core::Fceumm => "FCEUmm",
            Core::Nestopia => "Nestopia",
            Core::Snes9x => "Snes9x",
            Core::Snes9x2005 => "Snes9x 2005",
        }
    }

    pub fn parse(s: &str) -> Option<Core> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mgba" => Some(Core::Mgba),
            "gpsp" => Some(Core::Gpsp),
            "gambatte" => Some(Core::Gambatte),
            "fceumm" => Some(Core::Fceumm),
            "nestopia" => Some(Core::Nestopia),
            "snes9x" => Some(Core::Snes9x),
            "snes9x2005_plus" | "snes9x2005" => Some(Core::Snes9x2005),
            _ => None,
        }
    }

    pub fn runs(self, platform: Platform) -> bool {
        match self {
            Core::Mgba => matches!(platform, Platform::Gba | Platform::Gb | Platform::Gbc),
            Core::Gpsp => platform == Platform::Gba,
            Core::Gambatte => matches!(platform, Platform::Gb | Platform::Gbc),
            Core::Fceumm | Core::Nestopia => platform == Platform::Nes,
            Core::Snes9x | Core::Snes9x2005 => platform == Platform::Snes,
        }
    }

    /// The two cores the picker offers for a platform: the default on the left, the
    /// alternative right.
    pub fn pair(platform: Platform) -> [Core; 2] {
        match platform {
            Platform::Gba => [Core::Mgba, Core::Gpsp],
            Platform::Gb | Platform::Gbc => [Core::Mgba, Core::Gambatte],
            Platform::Nes => [Core::Fceumm, Core::Nestopia],
            Platform::Snes => [Core::Snes9x, Core::Snes9x2005],
        }
    }

    pub fn default_for(platform: Platform) -> Core {
        match platform {
            Platform::Gba | Platform::Gb | Platform::Gbc => Core::Mgba,
            Platform::Nes => Core::Fceumm,
            // The full Snes9x falls behind on the Flip in some games (Zombies Ate My Neighbors,
            // Uniracers), with no room left to fast-forward; 2005 Plus keeps up with headroom.
            Platform::Snes => Core::Snes9x2005,
        }
    }
}

pub fn read_selected_cores(root: &Path) -> HashMap<String, Core> {
    crate::ini::read(root, SELECTED_CORE_FILE)
        .into_iter()
        .filter_map(|(stem, name)| Core::parse(&name).map(|core| (stem, core)))
        .collect()
}

pub fn core_for(root: &Path, stem: &str) -> Core {
    crate::ini::value(root, SELECTED_CORE_FILE, stem)
        .as_deref()
        .and_then(Core::parse)
        .unwrap_or_default()
}

/// Prefixes a platform's folder name to make the key in selected_core.txt whose core every cart
/// of that platform without a pick of its own runs on, as in `*GBA=gpsp`.
pub const DEFAULT_CORE_KEY: &str = "*";

pub fn core_for_platform(root: &Path, stem: &str, platform: Platform) -> Core {
    let picked = |key: &str| {
        crate::ini::value(root, SELECTED_CORE_FILE, key)
            .as_deref()
            .and_then(Core::parse)
            .filter(|core| core.runs(platform))
    };
    picked(stem)
        .or_else(|| picked(&default_key(platform)))
        .unwrap_or_else(|| Core::default_for(platform))
}

/// The core a cart runs on: its own pick, else its folder's, else its platform's.
pub fn core_for_cart(root: &Path, cart: &crate::Cart) -> Core {
    let picked = |key: &str| {
        crate::ini::value(root, SELECTED_CORE_FILE, key)
            .as_deref()
            .and_then(Core::parse)
            .filter(|core| core.runs(cart.platform))
    };
    picked(&cart.stem)
        .or_else(|| {
            let folder = cart.folder()?;
            picked(&folder_core_key(&crate::folder_key(cart.platform, folder)))
        })
        .or_else(|| picked(&default_key(cart.platform)))
        .unwrap_or_else(|| Core::default_for(cart.platform))
}

/// A folder's own core, if it has one (`@GBA/Pokemon Hacks = mgba` in selected_core.txt).
pub fn folder_core(root: &Path, key: &str, platform: Platform) -> Option<Core> {
    crate::ini::value(root, SELECTED_CORE_FILE, &folder_core_key(key))
        .as_deref()
        .and_then(Core::parse)
        .filter(|core| core.runs(platform))
}

pub fn write_folder_core(root: &Path, key: &str, core: Option<Core>) -> std::io::Result<()> {
    crate::ini::put(
        root,
        SELECTED_CORE_FILE,
        &folder_core_key(key),
        core.map(|c| c.as_str()),
    )
}

fn folder_core_key(key: &str) -> String {
    format!("@{key}")
}

fn default_key(platform: Platform) -> String {
    format!("{DEFAULT_CORE_KEY}{}", platform.dir_name())
}

/// The core a platform's carts run on unless one picks its own.
pub fn default_core(root: &Path, platform: Platform) -> Core {
    crate::ini::value(root, SELECTED_CORE_FILE, &default_key(platform))
        .as_deref()
        .and_then(Core::parse)
        .filter(|core| core.runs(platform))
        .unwrap_or_else(|| Core::default_for(platform))
}

pub fn write_default_core(root: &Path, platform: Platform, core: Core) -> std::io::Result<()> {
    crate::ini::write(
        root,
        SELECTED_CORE_FILE,
        &default_key(platform),
        core.as_str(),
    )
}

pub fn write_selected_core(root: &Path, stem: &str, core: Core) -> std::io::Result<()> {
    crate::ini::write(root, SELECTED_CORE_FILE, stem, core.as_str())
}
