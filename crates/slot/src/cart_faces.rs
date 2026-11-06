use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use slot_gfx::{Compositor, TexId};
use slot_store::Cart;
use slot_ui::{cart_face_with, cart_interior, shows_interior, CartFace};

use crate::app::{App, ExtrasWeek, FaceWant, FolderSheet};
use crate::label_cache::label_art;

const WORKERS: usize = 2;

type Key = (usize, usize);

/// A cart's face and, for a clear GBA cart, the game-coloured inside it is drawn over, built for
/// the shelves as they stood at generation `u64`.
type Built = (Key, u64, CartFace, Option<CartFace>);

#[derive(Default)]
struct Queue {
    jobs: VecDeque<(Key, u64, Cart)>,
    closed: bool,
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

pub struct CartFaces {
    queue: Shared,
    built: Receiver<Built>,
    pending: HashSet<Key>,
    asked: Vec<FaceWant>,
    free: Vec<TexId>,
    /// The shelves' generation the queue was filled for; a re-sorted shelf moves its carts.
    epoch: u64,
}

impl CartFaces {
    pub fn spawn(root: PathBuf) -> Self {
        let queue: Shared = Arc::default();
        let (outbox, built) = mpsc::channel();
        for n in 0..WORKERS {
            let (queue, outbox, root) = (queue.clone(), outbox.clone(), root.clone());
            let spawned = thread::Builder::new()
                .name(format!("slot-carts-{n}"))
                .spawn(move || work(&queue, &outbox, &root));
            if let Err(e) = spawned {
                eprintln!("slot: cart faces: worker thread failed to start: {e}");
            }
        }
        CartFaces {
            queue,
            built,
            pending: HashSet::new(),
            asked: Vec::new(),
            free: Vec::new(),
            epoch: 0,
        }
    }

    pub fn sync(&mut self, app: &mut App, compositor: &mut Compositor) {
        self.sync_cart_faces(app, compositor);
        sync_folders(app, compositor);
        app.sync_extras_menu(compositor);
    }

    fn sync_cart_faces(&mut self, app: &mut App, compositor: &mut Compositor) {
        self.free.extend(app.shed_faces());
        if app.shelf_epoch() != self.epoch {
            self.epoch = app.shelf_epoch();
            self.pending.clear();
            self.asked.clear();
            let (lock, _) = &*self.queue;
            lock.lock().unwrap_or_else(|e| e.into_inner()).jobs.clear();
        }
        while let Ok((key, epoch, face, interior)) = self.built.try_recv() {
            self.place(app, compositor, key, epoch, face, interior);
        }
        let wants = app.face_wants();
        if wants != self.asked {
            self.requeue(app, &wants);
            self.asked = wants;
        }
        let mut waiting: Vec<Key> = self
            .asked
            .iter()
            .filter(|w| w.on_screen)
            .map(|w| (w.shelf, w.index))
            .collect();
        while !waiting.is_empty() {
            let Ok((key, epoch, face, interior)) = self.built.recv() else {
                return;
            };
            waiting.retain(|k| *k != key);
            self.place(app, compositor, key, epoch, face, interior);
        }
    }

    fn requeue(&mut self, app: &App, wants: &[FaceWant]) {
        let (lock, wake) = &*self.queue;
        let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
        for (key, _, _) in queue.jobs.drain(..) {
            self.pending.remove(&key);
        }
        for w in wants {
            let key = (w.shelf, w.index);
            if self.pending.contains(&key) {
                continue;
            }
            if let Some(cart) = app.face_cart(w.shelf, w.index) {
                queue.jobs.push_back((key, self.epoch, cart.clone()));
                self.pending.insert(key);
            }
        }
        wake.notify_all();
    }

    fn texture(&mut self, compositor: &mut Compositor, face: &CartFace) -> TexId {
        match self.free.pop() {
            Some(tex) => {
                compositor.update_texture(tex, face.w, face.h, &face.rgba);
                tex
            }
            None => compositor.create_texture(face.w, face.h, &face.rgba),
        }
    }

    fn place(
        &mut self,
        app: &mut App,
        compositor: &mut Compositor,
        key: Key,
        epoch: u64,
        face: CartFace,
        interior: Option<CartFace>,
    ) {
        if epoch != self.epoch {
            return;
        }
        self.pending.remove(&key);
        self.asked.retain(|w| (w.shelf, w.index) != key);
        let tex = self.texture(compositor, &face);
        self.free.extend(app.set_cart_face(key.0, key.1, tex));
        if let Some(interior) = interior {
            let tex = self.texture(compositor, &interior);
            self.free.extend(app.set_cart_interior(key.0, key.1, tex));
        }
    }
}

impl Drop for CartFaces {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        lock.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        wake.notify_all();
    }
}

fn work(queue: &Shared, outbox: &Sender<Built>, root: &Path) {
    let (lock, wake) = &**queue;
    loop {
        let (key, epoch, cart) = {
            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if q.closed {
                    return;
                }
                if let Some(job) = q.jobs.pop_front() {
                    break job;
                }
                q = wake.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        let face = cart_face_with(&cart, label_art(root, &cart));
        let interior = shows_interior(&cart).then(|| cart_interior(&cart));
        if outbox.send((key, epoch, face, interior)).is_err() {
            return;
        }
    }
}

/// A folder's paper and band, and an opened folder's name strip, are made once each; the stacks
/// then follow their carts' faces.
fn sync_folders(app: &mut App, compositor: &mut Compositor) {
    let mut upload =
        |face: slot_ui::CartFace| compositor.create_texture(face.w, face.h, &face.rgba);
    for (shelf, index, platform, paper, name) in app.folder_paper_wants() {
        if let Some(face) = slot_ui::folder_paper_face(platform, paper, &name) {
            app.set_folder_paper(shelf, index, upload(face));
        }
    }
    if app.wants_folder_band() {
        if let Some(face) = slot_ui::folder_band_face() {
            app.set_folder_band(upload(face));
        }
    }
    if let Some((shelf, name)) = app.folder_title_want() {
        if let Some(face) = slot_ui::folder_title_face(&name) {
            let w = face.w;
            app.set_folder_title(shelf, upload(face), w);
        }
    }
    if app.wants_folder_ui() {
        let menu_loop = upload(slot_ui::menu_loop_face());
        let key_loops = [false, true].map(|wide| upload(slot_ui::key_loop_face(wide)));
        let mut legend = |faces: Vec<slot_ui::UndoFace>| -> Vec<(TexId, u32)> {
            faces
                .into_iter()
                .map(|f| (compositor.create_texture(f.w, f.h, &f.rgba), f.w))
                .collect()
        };
        let folder = legend(slot_ui::folder_button_legends());
        let entry = legend(slot_ui::name_entry_legends());
        app.set_folder_ui(menu_loop, key_loops, folder, entry);
    }
    let mut upload =
        |face: slot_ui::CartFace| compositor.create_texture(face.w, face.h, &face.rgba);
    match app.folder_sheet_want() {
        Some(FolderSheet::Menu {
            paper,
            name,
            rows,
            first,
        }) => {
            if let Some(face) = slot_ui::folder_menu_face(paper, &name, &rows, first) {
                app.set_folder_sheet(false, upload(face));
            }
        }
        Some(FolderSheet::Entry { paper, name, lower }) => {
            if let Some(face) = slot_ui::name_entry_face(paper, &name, lower) {
                app.set_folder_sheet(true, upload(face));
            }
        }
        None => {}
    }
    if let Some(name) = app.game_delete_want() {
        let [keep, delete] = slot_ui::delete_legends(slot_ui::footer_ink(false));
        let faces = [
            slot_ui::menu_face(&format!("Delete {name}?")),
            slot_ui::menu_detail_face("Its saves and states are kept."),
            keep,
            delete,
        ]
        .map(|f| {
            let (w, h) = (f.w, f.h);
            let face = slot_ui::CartFace { rgba: f.rgba, w, h };
            (upload(face), w, h)
        });
        app.set_game_delete_faces(faces.into());
    }
    if app.wants_extra_parts() {
        let dice = [0, 1, 2].map(|die| slot_ui::die_face(die).map(&mut upload));
        let bubble = slot_ui::bubble_face().map(&mut upload);
        app.set_extra_parts(dice, bubble);
    }
    if let Some((week, waiting)) = app.extras_want() {
        let ExtrasWeek {
            range,
            days,
            labels,
            total,
        } = week;
        let receipt = slot_ui::receipt_face(&slot_ui::ReceiptWeek {
            range: &range,
            days,
            labels,
            total: &total,
        });
        let faces = [
            receipt,
            slot_ui::slate_face(),
            slot_ui::blister_face(waiting),
        ]
        .map(|face| face.map(&mut upload));
        app.set_extra_faces(faces);
    }
    if let Some((platform, time, when)) = app.played_plate_want() {
        if let Some(face) = slot_ui::played_face(platform, &time, &when) {
            let (w, h) = (face.w, face.h);
            app.set_played_plate(upload(face), w, h);
        }
    }
    if app.wants_report_frame() {
        let frame = upload(slot_ui::report_frame_face());
        let legend = slot_ui::report_legends()
            .into_iter()
            .map(|f| {
                (
                    upload(slot_ui::CartFace {
                        rgba: f.rgba,
                        w: f.w,
                        h: f.h,
                    }),
                    f.w,
                )
            })
            .collect();
        app.set_report_frame(frame, legend);
    }
    if let Some(sheet) = app.report_want() {
        if let Some(face) = slot_ui::report_face(&sheet) {
            let h = face.h;
            app.set_report_sheet(upload(face), h);
        }
    }
    app.refresh_folder_stacks();
}
