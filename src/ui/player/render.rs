use std::ffi::{c_char, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::{Arc, OnceLock};

use glib::translate::FromGlibPtrNone;
use gtk::prelude::*;
use gtk::GLArea;
use libloading::Library;

pub(super) const MPV_RENDER_PARAM_INVALID: u32 = 0;
pub(super) const MPV_RENDER_PARAM_API_TYPE: u32 = 1;
pub(super) const MPV_RENDER_PARAM_OPENGL_INIT_PARAMS: u32 = 2;
pub(super) const MPV_RENDER_PARAM_OPENGL_FBO: u32 = 3;
pub(super) const MPV_RENDER_PARAM_FLIP_Y: u32 = 4;

static EPOXY_LIB: OnceLock<Option<Library>> = OnceLock::new();
static EGL_LIB: OnceLock<Option<Library>> = OnceLock::new();
static GLX_LIB: OnceLock<Option<Library>> = OnceLock::new();

pub(super) unsafe extern "C" fn get_proc_address(
    _ctx: *mut c_void,
    name: *const c_char,
) -> *mut c_void {
    let epoxy = EPOXY_LIB.get_or_init(|| unsafe { Library::new("libepoxy.so.0").ok() });
    if let Some(lib) = epoxy {
        if let Ok(sym) = lib
            .get::<unsafe extern "C" fn(*const c_char) -> *mut c_void>(b"epoxy_get_proc_address\0")
        {
            let ptr = sym(name);
            if !ptr.is_null() {
                return ptr;
            }
        }
    }
    let egl = EGL_LIB.get_or_init(|| unsafe { Library::new("libEGL.so.1").ok() });
    if let Some(lib) = egl {
        if let Ok(sym) =
            lib.get::<unsafe extern "C" fn(*const c_char) -> *mut c_void>(b"eglGetProcAddress\0")
        {
            let ptr = sym(name);
            if !ptr.is_null() {
                return ptr;
            }
        }
    }
    let glx = GLX_LIB.get_or_init(|| unsafe { Library::new("libGL.so.1").ok() });
    if let Some(lib) = glx {
        if let Ok(sym) =
            lib.get::<unsafe extern "C" fn(*const c_char) -> *mut c_void>(b"glXGetProcAddressARB\0")
        {
            let ptr = sym(name);
            if !ptr.is_null() {
                return ptr;
            }
        }
    }
    ptr::null_mut()
}

pub(super) struct RenderSignal {
    pub(super) main_context: glib::MainContext,
    pub(super) pending: Arc<AtomicBool>,
    pub(super) active: Arc<AtomicBool>,
    pub(super) area: Arc<AtomicPtr<c_void>>,
}

pub(super) unsafe extern "C" fn update_callback(ctx: *mut c_void) {
    let signal = &*(ctx as *const RenderSignal);
    if !signal.active.load(Ordering::Acquire) || signal.pending.swap(true, Ordering::AcqRel) {
        return;
    }

    let pending = signal.pending.clone();
    let active = signal.active.clone();
    let area = signal.area.clone();
    let main_context = signal.main_context.clone();
    main_context.invoke(move || {
        pending.store(false, Ordering::Release);
        let area = area.load(Ordering::Acquire);
        if !active.load(Ordering::Acquire) || area.is_null() {
            return;
        }
        unsafe {
            let object: glib::Object = glib::Object::from_glib_none(area.cast());
            let area = object.unsafe_cast::<GLArea>();
            area.queue_render();
        }
    });
}
