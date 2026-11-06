use std::ffi::{c_void, CString};

use libloading::Library;

use crate::fbdev::{
    open, Egl, Ptr, EGL_ALPHA_SIZE, EGL_BLUE_SIZE, EGL_GREEN_SIZE, EGL_NONE, EGL_RED_SIZE,
    EGL_RENDERABLE_TYPE, EGL_SURFACE_TYPE,
};
use crate::surface::{GfxError, Surface};

const EGL_PLATFORM_SURFACELESS_MESA: u32 = 0x31DD;
const EGL_PBUFFER_BIT: i32 = 0x0001;
const EGL_WIDTH: i32 = 0x3057;
const EGL_HEIGHT: i32 = 0x3056;
const EGL_OPENGL_API: u32 = 0x30A2;
const EGL_OPENGL_BIT: i32 = 0x0008;
const EGL_CONTEXT_MAJOR_VERSION: i32 = 0x3098;
const EGL_CONTEXT_MINOR_VERSION: i32 = 0x30FB;
const EGL_CONTEXT_OPENGL_PROFILE_MASK: i32 = 0x30FD;
const EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT: i32 = 0x0001;

type GetPlatformDisplay = unsafe extern "C" fn(u32, Ptr, *const i32) -> Ptr;
type CreatePbufferSurface = unsafe extern "C" fn(Ptr, Ptr, *const i32) -> Ptr;

/// An off-screen OpenGL 3.3 core surface (as `headless.rs` uses on macOS) on Mesa's surfaceless platform, so a Linux PC without a panel can
/// draw slot's frames and read them back (`examples/record.rs`).
pub struct PbufferSurface {
    egl: Egl,
    gl: Library,
    display: Ptr,
    surface: Ptr,
    context: Ptr,
    size: (u32, u32),
}

impl PbufferSurface {
    pub fn new(size: (u32, u32)) -> Result<Self, GfxError> {
        let egl = Egl::load()?;
        let gl = open("libOpenGL.so.0").or_else(|_| open("libGL.so.1"))?;
        unsafe {
            let proc = |name: &std::ffi::CStr| (egl.get_proc_address)(name.as_ptr());
            let platform = proc(c"eglGetPlatformDisplayEXT");
            let display = if platform.is_null() {
                (egl.get_display)(std::ptr::null_mut())
            } else {
                let get: GetPlatformDisplay = std::mem::transmute(platform);
                get(
                    EGL_PLATFORM_SURFACELESS_MESA,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            if display.is_null()
                || (egl.initialize)(display, std::ptr::null_mut(), std::ptr::null_mut()) == 0
            {
                return Err(egl.fail("eglInitialize(surfaceless)"));
            }
            (egl.bind_api)(EGL_OPENGL_API);
            let attrs = [
                EGL_SURFACE_TYPE,
                EGL_PBUFFER_BIT,
                EGL_RENDERABLE_TYPE,
                EGL_OPENGL_BIT,
                EGL_RED_SIZE,
                8,
                EGL_GREEN_SIZE,
                8,
                EGL_BLUE_SIZE,
                8,
                EGL_ALPHA_SIZE,
                8,
                EGL_NONE,
            ];
            let mut config: Ptr = std::ptr::null_mut();
            let mut found = 0;
            if (egl.choose_config)(display, attrs.as_ptr(), &mut config, 1, &mut found) == 0
                || found == 0
            {
                return Err(egl.fail("eglChooseConfig(pbuffer)"));
            }
            let create = proc(c"eglCreatePbufferSurface");
            if create.is_null() {
                return Err(GfxError::Context("no eglCreatePbufferSurface".into()));
            }
            let create: CreatePbufferSurface = std::mem::transmute(create);
            let dims = [
                EGL_WIDTH,
                size.0 as i32,
                EGL_HEIGHT,
                size.1 as i32,
                EGL_NONE,
            ];
            let surface = create(display, config, dims.as_ptr());
            if surface.is_null() {
                return Err(egl.fail("eglCreatePbufferSurface"));
            }
            let context_attrs = [
                EGL_CONTEXT_MAJOR_VERSION,
                3,
                EGL_CONTEXT_MINOR_VERSION,
                3,
                EGL_CONTEXT_OPENGL_PROFILE_MASK,
                EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT,
                EGL_NONE,
            ];
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
            Ok(PbufferSurface {
                egl,
                gl,
                display,
                surface,
                context,
                size,
            })
        }
    }
}

impl Surface for PbufferSurface {
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
        unsafe { (self.egl.swap_buffers)(self.display, self.surface) };
        Ok(())
    }

    fn proc_address(&self, name: &str) -> *const c_void {
        let Ok(c) = CString::new(name) else {
            return std::ptr::null();
        };
        let exported = unsafe { self.gl.get::<unsafe extern "C" fn()>(name.as_bytes()) };
        match exported {
            Ok(f) => *f as *const c_void,
            Err(_) => unsafe { (self.egl.get_proc_address)(c.as_ptr()) },
        }
    }
}

impl Drop for PbufferSurface {
    fn drop(&mut self) {
        unsafe {
            (self.egl.make_current)(
                self.display,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            (self.egl.terminate)(self.display);
        }
    }
}
