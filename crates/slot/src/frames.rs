use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use slot_retro::{GBA_H, GBA_W};

pub struct Frames {
    inner: Mutex<Inner>,
    size: usize,
    taken: AtomicU64,
}

struct Inner {
    ready: Option<(Vec<u8>, (u32, u32))>,
    spare: Vec<Vec<u8>>,
    allocated: usize,
}

impl Frames {
    pub fn new(size: usize) -> Arc<Self> {
        Arc::new(Frames {
            inner: Mutex::new(Inner {
                ready: None,
                spare: Vec::new(),
                allocated: 0,
            }),
            size,
            taken: AtomicU64::new(0),
        })
    }

    pub fn take_write(&self) -> Vec<u8> {
        let mut i = self.lock();
        match i.spare.pop() {
            Some(buf) => buf,
            None => {
                i.allocated += 1;
                Vec::with_capacity(self.size)
            }
        }
    }

    pub fn publish(&self, buf: Vec<u8>) {
        self.publish_sized(buf, (GBA_W, GBA_H));
    }

    /// Publishes a frame of `size` (width, height) pixels, as the core sent it.
    pub fn publish_sized(&self, buf: Vec<u8>, size: (u32, u32)) {
        let mut i = self.lock();
        if let Some((dropped, _)) = i.ready.replace((buf, size)) {
            i.spare.push(dropped);
        }
    }

    pub fn latest(self: &Arc<Self>) -> Option<FrameRef> {
        let (buf, size) = self.lock().ready.take()?;
        self.taken.fetch_add(1, Ordering::Relaxed);
        Some(FrameRef {
            frames: self.clone(),
            buf,
            size,
        })
    }

    pub fn is_ready(&self) -> bool {
        self.lock().ready.is_some()
    }

    pub fn taken(&self) -> u64 {
        self.taken.load(Ordering::Relaxed)
    }

    pub fn allocated(&self) -> usize {
        self.lock().allocated
    }

    fn recycle(&self, buf: Vec<u8>) {
        self.lock().spare.push(buf);
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub struct FrameRef {
    frames: Arc<Frames>,
    buf: Vec<u8>,
    size: (u32, u32),
}

impl FrameRef {
    /// The frame's width and height in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }
}

impl Deref for FrameRef {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.buf
    }
}

impl Drop for FrameRef {
    fn drop(&mut self) {
        self.frames.recycle(std::mem::take(&mut self.buf));
    }
}

/// The game frame last put in the game texture. A paused or still game sends the same frame
/// again and again; the texture and the panel already show it, so it is not uploaded again,
/// and with nothing else moving the frame is not drawn or flipped either.
#[derive(Default)]
pub struct LastUpload {
    size: Option<(u32, u32)>,
    bytes: Vec<u8>,
}

impl LastUpload {
    /// True when `frame` of `size` differs from the last one, which it then becomes. A compare
    /// stops at the first differing byte, so a moving game pays little for it.
    pub fn changed(&mut self, frame: &[u8], size: (u32, u32)) -> bool {
        if self.size == Some(size) && self.bytes == frame {
            return false;
        }
        self.size = Some(size);
        self.bytes.clear();
        self.bytes.extend_from_slice(frame);
        true
    }
}
