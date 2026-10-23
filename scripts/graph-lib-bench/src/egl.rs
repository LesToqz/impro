//! Surfaceless EGL context on the default DRM render node (no window of any
//! kind). `LIBGL_ALWAYS_SOFTWARE=1` / `GALLIUM_DRIVER=llvmpipe` force Mesa's
//! software rasteriser.

use std::sync::Arc;

use khronos_egl as egl;

pub struct Egl {
    pub instance: egl::DynamicInstance<egl::EGL1_5>,
    pub display: egl::Display,
    pub context: egl::Context,
    pub gl: Arc<glow::Context>,
    pub info: String,
}

/// Opens a desktop-GL core context (`major.minor`) without any surface.
pub fn open(major: i32, minor: i32) -> Result<Egl, String> {
    use glow::HasContext;
    let instance = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required() }
        .map_err(|e| format!("load libEGL: {e}"))?;
    // EGL_PLATFORM_SURFACELESS_MESA
    let display = unsafe {
        instance.get_platform_display(0x31DD, egl::DEFAULT_DISPLAY, &[egl::ATTRIB_NONE])
    }
    .map_err(|e| format!("surfaceless display: {e}"))?;
    let (emaj, emin) = instance.initialize(display).map_err(|e| format!("eglInitialize: {e}"))?;
    instance.bind_api(egl::OPENGL_API).map_err(|e| format!("bind OpenGL: {e}"))?;
    let config = instance
        .choose_first_config(
            display,
            &[
                egl::SURFACE_TYPE,
                egl::PBUFFER_BIT,
                egl::RENDERABLE_TYPE,
                egl::OPENGL_BIT,
                egl::NONE,
            ],
        )
        .map_err(|e| format!("choose config: {e}"))?
        .ok_or("no EGL config")?;
    let context = instance
        .create_context(
            display,
            config,
            None,
            &[
                egl::CONTEXT_MAJOR_VERSION,
                major,
                egl::CONTEXT_MINOR_VERSION,
                minor,
                egl::CONTEXT_OPENGL_PROFILE_MASK,
                egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
                egl::NONE,
            ],
        )
        .map_err(|e| format!("create context {major}.{minor} core: {e}"))?;
    instance
        .make_current(display, None, None, Some(context))
        .map_err(|e| format!("make current (surfaceless): {e}"))?;
    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            instance
                .get_proc_address(s)
                .map_or(std::ptr::null(), |p| p as *const _)
        })
    };
    let info = unsafe {
        format!(
            "EGL {emaj}.{emin} vendor `{}`; GL_RENDERER `{}`; GL_VERSION `{}`; GLSL `{}`",
            instance.query_string(Some(display), egl::VENDOR).map(|c| c.to_string_lossy().into_owned()).unwrap_or_default(),
            gl.get_parameter_string(glow::RENDERER),
            gl.get_parameter_string(glow::VERSION),
            gl.get_parameter_string(glow::SHADING_LANGUAGE_VERSION),
        )
    };
    Ok(Egl {
        instance,
        display,
        context,
        gl: Arc::new(gl),
        info,
    })
}

impl Drop for Egl {
    fn drop(&mut self) {
        let _ = self.instance.make_current(self.display, None, None, None);
        let _ = self.instance.destroy_context(self.display, self.context);
        let _ = self.instance.terminate(self.display);
    }
}
