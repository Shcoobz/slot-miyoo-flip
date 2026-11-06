use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Platform {
    #[default]
    Gba,
    Gb,
    Gbc,
    Nes,
    Snes,
}

impl Platform {
    pub const ALL: [Platform; 5] = [
        Platform::Gba,
        Platform::Gb,
        Platform::Gbc,
        Platform::Nes,
        Platform::Snes,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Platform::Gba => "Game Boy Advance",
            Platform::Gb => "Game Boy",
            Platform::Gbc => "Game Boy Color",
            Platform::Nes => "NES",
            Platform::Snes => "Super Nintendo",
        }
    }

    pub fn dir_name(self) -> &'static str {
        match self {
            Platform::Gba => "GBA",
            Platform::Gb => "GB",
            Platform::Gbc => "GBC",
            Platform::Nes => "NES",
            Platform::Snes => "SNES",
        }
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Platform::Gba => &["gba"],
            Platform::Gb | Platform::Gbc => &["gb", "gbc"],
            Platform::Nes => &["nes"],
            Platform::Snes => &["sfc", "smc"],
        }
    }

    pub fn picture(self) -> (u32, u32) {
        match self {
            Platform::Gba => (240, 160),
            Platform::Gb | Platform::Gbc => (160, 144),
            // The picture as the cores usually send it; SNES hi-res games send up to 512x448.
            Platform::Nes => (256, 240),
            Platform::Snes => (256, 224),
        }
    }

    /// The home consoles, whose games were drawn for a 4:3 television rather than a handheld's
    /// own screen.
    pub fn on_a_tv(self) -> bool {
        matches!(self, Platform::Nes | Platform::Snes)
    }

    pub fn accepts(self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| {
                self.extensions()
                    .iter()
                    .any(|k| ext.eq_ignore_ascii_case(k))
            })
    }
}
