use std::sync::OnceLock;

use crate::bitmaps::{self, Mould, NES_CART_H, NES_CART_W, SNES_CART_H, SNES_CART_W};
use crate::cart::{CART_H, CART_W, GB_CART_H, GB_CART_W};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum GbShell {
    Notched,
    Rounded,
}

pub fn silhouette(w: u32, h: u32) -> Vec<u8> {
    alpha(bitmaps::gba_cart(), w, h)
}

pub fn gb_silhouette(shell: GbShell, w: u32, h: u32) -> Vec<u8> {
    let src = match shell {
        GbShell::Notched => bitmaps::gb_cart(),
        GbShell::Rounded => bitmaps::gbc_cart(),
    };
    alpha(src, w, h)
}

/// A mould's outline, from its front art's alpha, at the cart face's own size.
pub fn mould_silhouette(mould: Mould) -> Vec<u8> {
    match mould {
        Mould::Gba => silhouette(CART_W, CART_H),
        Mould::Notched => gb_silhouette(GbShell::Notched, GB_CART_W, GB_CART_H),
        Mould::Rounded => gb_silhouette(GbShell::Rounded, GB_CART_W, GB_CART_H),
        Mould::Nes => alpha(bitmaps::nes_cart(), NES_CART_W, NES_CART_H),
        Mould::Snes => alpha(bitmaps::snes_cart(), SNES_CART_W, SNES_CART_H),
    }
}

fn alpha(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    debug_assert_eq!(rgba.len(), (w * h * 4) as usize);
    rgba.iter().skip(3).step_by(4).copied().collect()
}

pub(crate) fn cart_mask() -> &'static [u8] {
    static MASK: OnceLock<Vec<u8>> = OnceLock::new();
    MASK.get_or_init(|| silhouette(CART_W, CART_H))
}

pub(crate) fn gb_cart_mask(shell: GbShell) -> &'static [u8] {
    static NOTCHED: OnceLock<Vec<u8>> = OnceLock::new();
    static ROUNDED: OnceLock<Vec<u8>> = OnceLock::new();
    let lock = match shell {
        GbShell::Notched => &NOTCHED,
        GbShell::Rounded => &ROUNDED,
    };
    lock.get_or_init(|| gb_silhouette(shell, GB_CART_W, GB_CART_H))
}

/// The cached outline of any mould.
pub(crate) fn mould_mask(mould: Mould) -> &'static [u8] {
    static NES: OnceLock<Vec<u8>> = OnceLock::new();
    static SNES: OnceLock<Vec<u8>> = OnceLock::new();
    match mould {
        Mould::Gba => cart_mask(),
        Mould::Notched => gb_cart_mask(GbShell::Notched),
        Mould::Rounded => gb_cart_mask(GbShell::Rounded),
        Mould::Nes => NES.get_or_init(|| mould_silhouette(Mould::Nes)),
        Mould::Snes => SNES.get_or_init(|| mould_silhouette(Mould::Snes)),
    }
}
