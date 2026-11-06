//! Draws slot off-screen on a Linux PC and saves chosen frames as PNG, to check a screen without a
//! handheld: `shots ROOT SCRIPT OUT_DIR [WxH]`. The script is `record.rs`'s (`wait N`, `tap BTN`,
//! `down BTN`, `up BTN`) plus `shot NAME`, which writes `OUT_DIR/NAME.png`.

use slot::frontend::Frontend;
use slot_gfx::{Compositor, PbufferSurface};
use slot_input::{Btn, InputSource, Millis, RawEvent};
use slot_power::SimPlatform;

const FPS: f64 = 60.0;

const CLOCK: i64 = 1_790_784_000;

enum Step {
    Wait(u32),
    Down(Btn),
    Up(Btn),
    Shot(String),
}

fn btn(name: &str) -> Btn {
    match name {
        "up" => Btn::Up,
        "down" => Btn::Down,
        "left" => Btn::Left,
        "right" => Btn::Right,
        "a" => Btn::A,
        "b" => Btn::B,
        "x" => Btn::X,
        "y" => Btn::Y,
        "l1" => Btn::L1,
        "r1" => Btn::R1,
        "start" => Btn::Start,
        "select" => Btn::Select,
        "menu" => Btn::Menu,
        other => panic!("unknown button {other}"),
    }
}

fn parse(script: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    for line in script.lines() {
        let words: Vec<&str> = line.split('#').next().unwrap().split_whitespace().collect();
        let n = |i: usize, default: u32| words.get(i).map_or(default, |w| w.parse().unwrap());
        match words.as_slice() {
            [] => {}
            ["wait", ..] => steps.push(Step::Wait(n(1, 1))),
            ["tap", b, ..] => {
                steps.push(Step::Down(btn(b)));
                steps.push(Step::Wait(n(2, 6)));
                steps.push(Step::Up(btn(b)));
            }
            ["down", b] => steps.push(Step::Down(btn(b))),
            ["up", b] => steps.push(Step::Up(btn(b))),
            ["shot", name] => steps.push(Step::Shot(name.to_string())),
            _ => panic!("cannot read {line:?}"),
        }
    }
    steps
}

#[derive(Default)]
struct Scripted(Vec<RawEvent>);

impl InputSource for Scripted {
    fn poll(&mut self, _now: Millis) -> Vec<RawEvent> {
        std::mem::take(&mut self.0)
    }
}

fn save(path: &str, (w, h): (u32, u32), rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("create the png");
    let mut png = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    let mut writer = png.write_header().expect("png header");
    let opaque: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2], 255])
        .collect();
    writer.write_image_data(&opaque).expect("png data");
    eprintln!("shots: {path}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (root, script, out, size) = match args.as_slice() {
        [root, script, out] => (root, script, out, "640x480"),
        [root, script, out, size] => (root, script, out, size.as_str()),
        _ => {
            eprintln!("usage: shots ROOT SCRIPT OUT_DIR [WxH]");
            std::process::exit(2);
        }
    };
    let (w, h) = size.split_once('x').expect("WxH");
    let window: (u32, u32) = (w.parse().unwrap(), h.parse().unwrap());
    let steps = parse(&std::fs::read_to_string(script).expect("read the script"));
    let surface = PbufferSurface::new(window).expect("surfaceless EGL");
    let mut compositor = Compositor::new(&surface).expect("compositor");
    let mut frontend = Frontend::boot(Box::new(SimPlatform::at(root.into()).stopped_at(CLOCK)));
    frontend.upload_faces(&mut compositor);
    frontend.upload_bezel(&mut compositor, window);
    frontend.drive_emulator();
    let mut input = Scripted::default();
    let mut frame = 0u64;
    for step in steps {
        match step {
            Step::Wait(n) => {
                for _ in 0..n {
                    run(
                        &mut frontend,
                        &mut compositor,
                        &mut input,
                        &mut frame,
                        window,
                    );
                }
            }
            Step::Down(b) => input.0.push(RawEvent::Down(b)),
            Step::Up(b) => input.0.push(RawEvent::Up(b)),
            Step::Shot(name) => {
                run(
                    &mut frontend,
                    &mut compositor,
                    &mut input,
                    &mut frame,
                    window,
                );
                let app = frontend.app();
                eprintln!(
                    "shots: dozing {} brightness {} canvas {:?}",
                    app.dozing(),
                    app.brightness(),
                    app.canvas()
                );
                // An unchanged frame is skipped, which leaves the canvas cleared: draw it again.
                frontend.compose(&mut compositor);
                let rgba = compositor.read_window(window);
                save(&format!("{out}/{name}.png"), window, &rgba);
            }
        }
    }
}

fn run(
    frontend: &mut Frontend,
    compositor: &mut Compositor,
    input: &mut Scripted,
    frame: &mut u64,
    window: (u32, u32),
) {
    *frame += 1;
    let now = (*frame as f64 * 1000.0 / FPS) as Millis;
    frontend.advance_at(input, now, (1.0 / FPS) as f32);
    let tick = std::time::Duration::from_secs_f64(1.0 / FPS);
    frontend.step_emulator(tick, std::time::Duration::from_secs(1));
    // Faces are built on a worker thread; give it a moment so a shot is not of blank carts.
    std::thread::sleep(std::time::Duration::from_millis(2));
    frontend.render(compositor, window);
}
