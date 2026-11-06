use std::ffi::{c_char, c_void, CString};
use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;

use libloading::Library;

use crate::fbdev::{
    open, sym, Egl, Ptr, EGL_ALPHA_SIZE, EGL_BLUE_SIZE, EGL_CONTEXT_CLIENT_VERSION, EGL_GREEN_SIZE,
    EGL_NONE, EGL_OPENGL_ES2_BIT, EGL_OPENGL_ES_API, EGL_RED_SIZE, EGL_RENDERABLE_TYPE,
    EGL_SURFACE_TYPE, EGL_WINDOW_BIT,
};
use crate::surface::{GfxError, Surface};

const CARD: &str = "/dev/dri/card0";

const EGL_NATIVE_VISUAL_ID: i32 = 0x302E;
const EGL_PLATFORM_GBM_KHR: u32 = 0x31D7;
const GBM_FORMAT_XRGB8888: u32 = 0x3432_5258;
const GBM_BO_USE_SCANOUT: u32 = 1 << 0;
const GBM_BO_USE_RENDERING: u32 = 1 << 2;
const DRM_MODE_CONNECTED: u32 = 1;
const DRM_MODE_CONNECTOR_HDMIA: u32 = 11;
const DRM_MODE_TYPE_PREFERRED: u32 = 1 << 3;
const DRM_MODE_PAGE_FLIP_EVENT: u32 = 1;
const DRM_EVENT_CONTEXT_VERSION: i32 = 2;
const POLLIN: i16 = 1;
const FLIP_TIMEOUT_MS: i32 = 1000;
const MAX_CONFIGS: usize = 64;

#[repr(C)]
#[derive(Clone, Copy)]
struct ModeInfo {
    clock: u32,
    hdisplay: u16,
    hsync_start: u16,
    hsync_end: u16,
    htotal: u16,
    hskew: u16,
    vdisplay: u16,
    vsync_start: u16,
    vsync_end: u16,
    vtotal: u16,
    vscan: u16,
    vrefresh: u32,
    flags: u32,
    kind: u32,
    name: [c_char; 32],
}

#[repr(C)]
struct Resources {
    count_fbs: i32,
    fbs: *const u32,
    count_crtcs: i32,
    crtcs: *const u32,
    count_connectors: i32,
    connectors: *const u32,
    count_encoders: i32,
    encoders: *const u32,
    min_width: u32,
    max_width: u32,
    min_height: u32,
    max_height: u32,
}

#[repr(C)]
struct Connector {
    connector_id: u32,
    encoder_id: u32,
    connector_type: u32,
    connector_type_id: u32,
    connection: u32,
    mm_width: u32,
    mm_height: u32,
    subpixel: u32,
    count_modes: i32,
    modes: *const ModeInfo,
    count_props: i32,
    props: *const u32,
    prop_values: *const u64,
    count_encoders: i32,
    encoders: *const u32,
}

#[repr(C)]
struct Encoder {
    encoder_id: u32,
    encoder_type: u32,
    crtc_id: u32,
    possible_crtcs: u32,
    possible_clones: u32,
}

#[repr(C)]
struct Crtc {
    crtc_id: u32,
    buffer_id: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    mode_valid: i32,
    mode: ModeInfo,
    gamma_size: i32,
}

type FlipHandler = unsafe extern "C" fn(i32, u32, u32, u32, *mut c_void);
type GetPlatformDisplay = unsafe extern "C" fn(u32, Ptr, *const i32) -> Ptr;

#[repr(C)]
struct EventContext {
    version: i32,
    vblank_handler: Option<FlipHandler>,
    page_flip_handler: Option<FlipHandler>,
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn poll(fds: *mut PollFd, nfds: std::ffi::c_ulong, timeout: i32) -> i32;
}

struct Drm {
    get_resources: unsafe extern "C" fn(i32) -> *mut Resources,
    free_resources: unsafe extern "C" fn(*mut Resources),
    get_connector: unsafe extern "C" fn(i32, u32) -> *mut Connector,
    free_connector: unsafe extern "C" fn(*mut Connector),
    get_encoder: unsafe extern "C" fn(i32, u32) -> *mut Encoder,
    free_encoder: unsafe extern "C" fn(*mut Encoder),
    get_crtc: unsafe extern "C" fn(i32, u32) -> *mut Crtc,
    free_crtc: unsafe extern "C" fn(*mut Crtc),
    add_fb: unsafe extern "C" fn(i32, u32, u32, u8, u8, u32, u32, *mut u32) -> i32,
    rm_fb: unsafe extern "C" fn(i32, u32) -> i32,
    set_crtc:
        unsafe extern "C" fn(i32, u32, u32, u32, u32, *const u32, i32, *const ModeInfo) -> i32,
    page_flip: unsafe extern "C" fn(i32, u32, u32, u32, *mut c_void) -> i32,
    handle_event: unsafe extern "C" fn(i32, *mut EventContext) -> i32,
    _lib: Library,
}

impl Drm {
    fn load() -> Result<Self, GfxError> {
        let lib = open("libdrm.so.2").or_else(|_| open("libdrm.so"))?;
        unsafe {
            Ok(Drm {
                get_resources: sym(&lib, "drmModeGetResources")?,
                free_resources: sym(&lib, "drmModeFreeResources")?,
                get_connector: sym(&lib, "drmModeGetConnector")?,
                free_connector: sym(&lib, "drmModeFreeConnector")?,
                get_encoder: sym(&lib, "drmModeGetEncoder")?,
                free_encoder: sym(&lib, "drmModeFreeEncoder")?,
                get_crtc: sym(&lib, "drmModeGetCrtc")?,
                free_crtc: sym(&lib, "drmModeFreeCrtc")?,
                add_fb: sym(&lib, "drmModeAddFB")?,
                rm_fb: sym(&lib, "drmModeRmFB")?,
                set_crtc: sym(&lib, "drmModeSetCrtc")?,
                page_flip: sym(&lib, "drmModePageFlip")?,
                handle_event: sym(&lib, "drmHandleEvent")?,
                _lib: lib,
            })
        }
    }
}

struct Gbm {
    create_device: unsafe extern "C" fn(i32) -> Ptr,
    device_destroy: unsafe extern "C" fn(Ptr),
    surface_create: unsafe extern "C" fn(Ptr, u32, u32, u32, u32) -> Ptr,
    surface_destroy: unsafe extern "C" fn(Ptr),
    lock_front_buffer: unsafe extern "C" fn(Ptr) -> Ptr,
    release_buffer: unsafe extern "C" fn(Ptr, Ptr),
    bo_get_handle: unsafe extern "C" fn(Ptr) -> u64,
    bo_get_stride: unsafe extern "C" fn(Ptr) -> u32,
    _lib: Library,
}

impl Gbm {
    fn load() -> Result<Self, GfxError> {
        let lib = open("libgbm.so.1").or_else(|_| open("libgbm.so"))?;
        unsafe {
            Ok(Gbm {
                create_device: sym(&lib, "gbm_create_device")?,
                device_destroy: sym(&lib, "gbm_device_destroy")?,
                surface_create: sym(&lib, "gbm_surface_create")?,
                surface_destroy: sym(&lib, "gbm_surface_destroy")?,
                lock_front_buffer: sym(&lib, "gbm_surface_lock_front_buffer")?,
                release_buffer: sym(&lib, "gbm_surface_release_buffer")?,
                bo_get_handle: sym(&lib, "gbm_bo_get_handle")?,
                bo_get_stride: sym(&lib, "gbm_bo_get_stride")?,
                _lib: lib,
            })
        }
    }
}

struct Output {
    connector: u32,
    crtc: u32,
    mode: ModeInfo,
}

pub struct KmsSurface {
    egl: Egl,
    gles: Library,
    drm: Drm,
    gbm: Gbm,
    card: File,
    output: Output,
    saved: *mut Crtc,
    device: Ptr,
    window: Ptr,
    display: Ptr,
    surface: Ptr,
    context: Ptr,
    size: (u32, u32),
    fbs: Vec<(Ptr, u32)>,
    front: Ptr,
    queued: Ptr,
    flipped: Box<bool>,
    shown: bool,
}

fn context_error(what: &str) -> GfxError {
    GfxError::Context(format!("kms: {what}"))
}

unsafe fn find_output(drm: &Drm, fd: i32) -> Result<Output, GfxError> {
    let res = (drm.get_resources)(fd);
    if res.is_null() {
        return Err(context_error("drmModeGetResources failed"));
    }
    let r = &*res;
    let connectors = std::slice::from_raw_parts(r.connectors, r.count_connectors.max(0) as usize);
    let crtcs = std::slice::from_raw_parts(r.crtcs, r.count_crtcs.max(0) as usize);
    // A connected TV wins over the built-in panel; otherwise the first connected output.
    let mut order: Vec<(bool, u32)> = Vec::new();
    for &id in connectors {
        let conn = (drm.get_connector)(fd, id);
        if conn.is_null() {
            continue;
        }
        let c = &*conn;
        if c.connection == DRM_MODE_CONNECTED && c.count_modes > 0 {
            order.push((c.connector_type != DRM_MODE_CONNECTOR_HDMIA, id));
        }
        (drm.free_connector)(conn);
    }
    order.sort();
    let mut found = None;
    for (_, id) in order {
        let conn = (drm.get_connector)(fd, id);
        if conn.is_null() {
            continue;
        }
        let c = &*conn;
        if c.connection == DRM_MODE_CONNECTED && c.count_modes > 0 {
            let modes = std::slice::from_raw_parts(c.modes, c.count_modes as usize);
            let mode = *modes
                .iter()
                .find(|m| m.kind & DRM_MODE_TYPE_PREFERRED != 0)
                .unwrap_or(&modes[0]);
            let mut crtc = 0;
            if c.encoder_id != 0 {
                let enc = (drm.get_encoder)(fd, c.encoder_id);
                if !enc.is_null() {
                    crtc = (*enc).crtc_id;
                    (drm.free_encoder)(enc);
                }
            }
            if crtc == 0 {
                crtc = free_crtc(drm, fd, c, crtcs);
            }
            found = Some(Output {
                connector: id,
                crtc,
                mode,
            });
        }
        (drm.free_connector)(conn);
        if found.is_some() {
            break;
        }
    }
    (drm.free_resources)(res);
    match found {
        Some(o) if o.crtc != 0 => Ok(o),
        Some(_) => Err(context_error("connected panel has no crtc")),
        None => Err(context_error("no connected panel")),
    }
}

/// A CRTC one of the connector's encoders can drive, for an output nothing drives yet.
unsafe fn free_crtc(drm: &Drm, fd: i32, c: &Connector, crtcs: &[u32]) -> u32 {
    let encoders = std::slice::from_raw_parts(c.encoders, c.count_encoders.max(0) as usize);
    for &id in encoders {
        let enc = (drm.get_encoder)(fd, id);
        if enc.is_null() {
            continue;
        }
        let possible = (*enc).possible_crtcs;
        (drm.free_encoder)(enc);
        if let Some(&crtc) = crtcs
            .iter()
            .enumerate()
            .find(|(i, _)| possible >> i & 1 == 1)
            .map(|(_, c)| c)
        {
            return crtc;
        }
    }
    crtcs.first().copied().unwrap_or(0)
}

unsafe fn choose_config(egl: &Egl, display: Ptr) -> Result<(Ptr, u32), GfxError> {
    let attrs = [
        EGL_SURFACE_TYPE,
        EGL_WINDOW_BIT,
        EGL_RED_SIZE,
        8,
        EGL_GREEN_SIZE,
        8,
        EGL_BLUE_SIZE,
        8,
        EGL_ALPHA_SIZE,
        0,
        EGL_RENDERABLE_TYPE,
        EGL_OPENGL_ES2_BIT,
        EGL_NONE,
    ];
    let mut configs: [Ptr; MAX_CONFIGS] = [std::ptr::null_mut(); MAX_CONFIGS];
    let mut count = 0;
    if (egl.choose_config)(
        display,
        attrs.as_ptr(),
        configs.as_mut_ptr(),
        MAX_CONFIGS as i32,
        &mut count,
    ) == 0
    {
        return Err(egl.fail("eglChooseConfig"));
    }
    let configs = &configs[..count.max(0) as usize];
    let visual = |config: Ptr| {
        let mut id = 0;
        (egl.get_config_attrib)(display, config, EGL_NATIVE_VISUAL_ID, &mut id);
        id as u32
    };
    let config = configs
        .iter()
        .copied()
        .find(|c| visual(*c) == GBM_FORMAT_XRGB8888)
        .or_else(|| configs.first().copied())
        .ok_or_else(|| context_error("eglChooseConfig: no es2 window config with 8/8/8 colour"))?;
    let format = match visual(config) {
        0 => GBM_FORMAT_XRGB8888,
        id => id,
    };
    Ok((config, format))
}

unsafe fn gbm_display(egl: &Egl, device: Ptr) -> Ptr {
    let name = c"eglGetPlatformDisplayEXT";
    let ext = (egl.get_proc_address)(name.as_ptr());
    if !ext.is_null() {
        let get: GetPlatformDisplay = std::mem::transmute(ext);
        let display = get(EGL_PLATFORM_GBM_KHR, device, std::ptr::null());
        if !display.is_null() {
            return display;
        }
    }
    (egl.get_display)(device)
}

unsafe extern "C" fn flipped(_fd: i32, _seq: u32, _sec: u32, _usec: u32, data: *mut c_void) {
    *(data as *mut bool) = true;
}

impl KmsSurface {
    pub fn new() -> Result<Self, GfxError> {
        let egl = Egl::load()?;
        let gles = open("libGLESv2.so.2").or_else(|_| open("libGLESv2.so"))?;
        let gbm = Gbm::load()?;
        let drm = Drm::load()?;
        let card = OpenOptions::new()
            .read(true)
            .write(true)
            .open(CARD)
            .map_err(|e| context_error(&format!("{CARD}: {e}")))?;
        let fd = card.as_raw_fd();
        unsafe {
            let output = find_output(&drm, fd)?;
            let (w, h) = (output.mode.hdisplay as u32, output.mode.vdisplay as u32);
            eprintln!(
                "slot: kms connector {} crtc {} mode {w}x{h}@{}",
                output.connector, output.crtc, output.mode.vrefresh
            );
            let saved = (drm.get_crtc)(fd, output.crtc);
            let device = (gbm.create_device)(fd);
            if device.is_null() {
                return Err(context_error("gbm_create_device failed"));
            }
            let display = gbm_display(&egl, device);
            if display.is_null() {
                return Err(egl.fail("eglGetPlatformDisplayEXT(gbm)"));
            }
            if (egl.initialize)(display, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
                return Err(egl.fail("eglInitialize"));
            }
            if (egl.bind_api)(EGL_OPENGL_ES_API) == 0 {
                return Err(egl.fail("eglBindAPI"));
            }
            let (config, format) = choose_config(&egl, display)?;
            let window = (gbm.surface_create)(
                device,
                w,
                h,
                format,
                GBM_BO_USE_SCANOUT | GBM_BO_USE_RENDERING,
            );
            if window.is_null() {
                return Err(context_error("gbm_surface_create failed"));
            }
            let surface = (egl.create_window_surface)(display, config, window, std::ptr::null());
            if surface.is_null() {
                return Err(egl.fail("eglCreateWindowSurface(gbm)"));
            }
            let context_attrs = [EGL_CONTEXT_CLIENT_VERSION, 2, EGL_NONE];
            let context = (egl.create_context)(
                display,
                config,
                std::ptr::null_mut(),
                context_attrs.as_ptr(),
            );
            if context.is_null() {
                return Err(egl.fail("eglCreateContext"));
            }
            if (egl.make_current)(display, surface, surface, context) == 0 {
                return Err(egl.fail("eglMakeCurrent"));
            }
            Ok(KmsSurface {
                egl,
                gles,
                drm,
                gbm,
                card,
                output,
                saved,
                device,
                window,
                display,
                surface,
                context,
                size: (w, h),
                fbs: Vec::new(),
                front: std::ptr::null_mut(),
                queued: std::ptr::null_mut(),
                flipped: Box::new(false),
                shown: false,
            })
        }
    }

    fn fd(&self) -> i32 {
        self.card.as_raw_fd()
    }

    fn fb_for(&mut self, bo: Ptr) -> Result<u32, GfxError> {
        if let Some((_, fb)) = self.fbs.iter().find(|(b, _)| *b == bo) {
            return Ok(*fb);
        }
        let mut fb = 0;
        let rc = unsafe {
            let handle = (self.gbm.bo_get_handle)(bo) as u32;
            let stride = (self.gbm.bo_get_stride)(bo);
            (self.drm.add_fb)(
                self.fd(),
                self.size.0,
                self.size.1,
                24,
                32,
                stride,
                handle,
                &mut fb,
            )
        };
        if rc != 0 {
            return Err(context_error(&format!("drmModeAddFB failed: {rc}")));
        }
        self.fbs.push((bo, fb));
        Ok(fb)
    }

    /// Shows `bo` without waiting for it: the first frame by mode set, every later one by a page
    /// flip that lands on the next vblank. slot's pacer expects swap to return at once, as the
    /// fbdev driver's does, so the wait for a flip happens at the next swap instead.
    fn show(&mut self, bo: Ptr) -> Result<(), GfxError> {
        let fb = self.fb_for(bo)?;
        let fd = self.fd();
        if !self.shown {
            let rc = unsafe {
                (self.drm.set_crtc)(
                    fd,
                    self.output.crtc,
                    fb,
                    0,
                    0,
                    &self.output.connector,
                    1,
                    &self.output.mode,
                )
            };
            if rc != 0 {
                return Err(context_error(&format!("drmModeSetCrtc failed: {rc}")));
            }
            self.shown = true;
            self.front = bo;
            return Ok(());
        }
        *self.flipped = false;
        let done: *mut bool = &mut *self.flipped;
        let rc = unsafe {
            (self.drm.page_flip)(
                fd,
                self.output.crtc,
                fb,
                DRM_MODE_PAGE_FLIP_EVENT,
                done as *mut c_void,
            )
        };
        if rc != 0 {
            return Err(context_error(&format!("drmModePageFlip failed: {rc}")));
        }
        self.queued = bo;
        Ok(())
    }

    /// Waits for the flip in flight, then hands the buffer it replaced back to gbm.
    fn settle(&mut self) -> Result<(), GfxError> {
        if self.queued.is_null() {
            return Ok(());
        }
        let fd = self.fd();
        let mut events = EventContext {
            version: DRM_EVENT_CONTEXT_VERSION,
            vblank_handler: None,
            page_flip_handler: Some(flipped),
        };
        while !*self.flipped {
            let mut pfd = PollFd {
                fd,
                events: POLLIN,
                revents: 0,
            };
            if unsafe { poll(&mut pfd, 1, FLIP_TIMEOUT_MS) } <= 0 {
                return Err(context_error("page flip timed out"));
            }
            unsafe { (self.drm.handle_event)(fd, &mut events) };
        }
        let shown = std::mem::replace(&mut self.queued, std::ptr::null_mut());
        let gone = std::mem::replace(&mut self.front, shown);
        if !gone.is_null() {
            unsafe { (self.gbm.release_buffer)(self.window, gone) };
        }
        Ok(())
    }
}

impl Surface for KmsSurface {
    fn make_current(&mut self) -> Result<(), GfxError> {
        let ok = unsafe {
            (self.egl.make_current)(self.display, self.surface, self.surface, self.context)
        };
        match ok {
            0 => Err(self.egl.fail("eglMakeCurrent")),
            _ => Ok(()),
        }
    }

    fn window_size(&self) -> (u32, u32) {
        self.size
    }

    fn swap(&mut self) -> Result<(), GfxError> {
        self.settle()?;
        if unsafe { (self.egl.swap_buffers)(self.display, self.surface) } == 0 {
            return Err(self.egl.fail("eglSwapBuffers"));
        }
        let bo = unsafe { (self.gbm.lock_front_buffer)(self.window) };
        if bo.is_null() {
            return Err(context_error("gbm_surface_lock_front_buffer failed"));
        }
        let shown = self.show(bo);
        if shown.is_err() {
            unsafe { (self.gbm.release_buffer)(self.window, bo) };
        }
        shown
    }

    fn paces_itself(&self) -> bool {
        true
    }

    fn proc_address(&self, name: &str) -> *const c_void {
        let Ok(c) = CString::new(name) else {
            return std::ptr::null();
        };
        let exported = unsafe { self.gles.get::<unsafe extern "C" fn()>(name.as_bytes()) };
        match exported {
            Ok(f) => *f as *const c_void,
            Err(_) => unsafe { (self.egl.get_proc_address)(c.as_ptr()) },
        }
    }
}

impl Drop for KmsSurface {
    fn drop(&mut self) {
        let _ = self.settle();
        let fd = self.fd();
        unsafe {
            if !self.saved.is_null() {
                let s = &*self.saved;
                if s.mode_valid != 0 {
                    (self.drm.set_crtc)(
                        fd,
                        s.crtc_id,
                        s.buffer_id,
                        s.x,
                        s.y,
                        &self.output.connector,
                        1,
                        &s.mode,
                    );
                }
                (self.drm.free_crtc)(self.saved);
            }
            for (_, fb) in self.fbs.drain(..) {
                (self.drm.rm_fb)(fd, fb);
            }
            for bo in [self.queued, self.front] {
                if !bo.is_null() {
                    (self.gbm.release_buffer)(self.window, bo);
                }
            }
            (self.egl.make_current)(
                self.display,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            (self.egl.terminate)(self.display);
            (self.gbm.surface_destroy)(self.window);
            (self.gbm.device_destroy)(self.device);
        }
    }
}
