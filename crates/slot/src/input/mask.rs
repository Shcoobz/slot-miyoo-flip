use slot_input::{Action, Btn};
use slot_retro::ButtonMask;
use slot_store::{Controls, GameButton, Modifier, Platform};

use super::bind::device_btn;

#[derive(Default)]
pub struct Pad {
    held: Vec<Btn>,
    chorded: Vec<Btn>,
}

impl Pad {
    pub fn apply(&mut self, action: Action) {
        let (list, b, down) = match action {
            Action::GbaDown(b) => (&mut self.held, b, true),
            Action::GbaUp(b) => (&mut self.held, b, false),
            Action::MenuChordDown(b) => (&mut self.chorded, b, true),
            Action::MenuChordUp(b) => (&mut self.chorded, b, false),
            _ => return,
        };
        list.retain(|x| *x != b);
        if down {
            list.push(b);
        }
    }

    /// The pad as the playing console sees it: only its own buttons count, so the Flip's X and
    /// Y are turbo on most consoles and the Super Nintendo's X and Y on that one.
    pub fn mask(&self, controls: &Controls, platform: Platform) -> ButtonMask {
        let bits = GameButton::of(platform)
            .iter()
            .copied()
            .filter(|g| {
                controls.game(*g).is_some_and(|binding| {
                    let b = device_btn(binding.button);
                    match binding.modifier {
                        None => self.held.contains(&b),
                        Some(Modifier::Menu) => self.chorded.contains(&b),
                        Some(Modifier::Select | Modifier::Button(_)) => false,
                    }
                })
            })
            .fold(0, |bits, g| bits | bit(g));
        ButtonMask(bits)
    }

    pub fn clear(&mut self) {
        self.held.clear();
        self.chorded.clear();
    }
}

fn bit(g: GameButton) -> u16 {
    match g {
        GameButton::Up => ButtonMask::UP,
        GameButton::Down => ButtonMask::DOWN,
        GameButton::Left => ButtonMask::LEFT,
        GameButton::Right => ButtonMask::RIGHT,
        GameButton::A => ButtonMask::A,
        GameButton::B => ButtonMask::B,
        GameButton::Select => ButtonMask::SELECT,
        GameButton::Start => ButtonMask::START,
        GameButton::L => ButtonMask::L,
        GameButton::R => ButtonMask::R,
        GameButton::TurboA => ButtonMask::TURBO_A,
        GameButton::TurboB => ButtonMask::TURBO_B,
        GameButton::X => ButtonMask::X,
        GameButton::Y => ButtonMask::Y,
    }
}
