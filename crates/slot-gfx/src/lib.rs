macro_rules! eprintln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

mod bezel;
mod draw;
mod fbdev;
mod fbo;
mod gl;
mod grade;
#[cfg(target_os = "macos")]
mod headless;
#[cfg(feature = "host")]
mod host;
#[cfg(target_os = "linux")]
mod kms;
mod layout;
mod lcd3x;
#[cfg(target_os = "linux")]
mod pbuffer;
mod pipeline;
mod power;
mod quad;
mod shaders;
mod surface;

pub use bezel::{framed, game_rect, lifted_rect, screen_rect, tall_rect, tv_dots, tv_rect, TvFit};
pub use draw::{Draw, TexId};
pub use fbdev::{egl_error, panel_mode, panel_size, FbdevSurface};
pub use fbo::{canvas_size, Compositor, BACKDROP};
pub use grade::{blue_light_gain, BLUE_LIGHT_MAX};
#[cfg(target_os = "macos")]
pub use headless::HeadlessSurface;
#[cfg(feature = "host")]
pub use host::HostSurface;
#[cfg(target_os = "linux")]
pub use kms::KmsSurface;
pub use layout::Canvas;
pub use lcd3x::{lcd3x_factors, lcd3x_mask, ScreenEffect};
#[cfg(target_os = "linux")]
pub use pbuffer::PbufferSurface;
pub use pipeline::{GAME_MAX_H, GAME_MAX_W, SCALE, SRC_H, SRC_W, WHOLE_TEXTURE};
pub use power::{screen_brightness, screen_rect_above, screen_scale, screen_width};
pub use surface::{blit_rect, blit_rect_fit, fit_rect, fit_scale, GfxError, Surface, OUT_H, OUT_W};
