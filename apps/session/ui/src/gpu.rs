//! What the window's GPU device asks for.
//!
//! The renderer (Vello, through `wgpu_context`) asks for wgpu's default
//! limits, whose largest storage buffer is 128 MiB. A long song's
//! arrangement — Unbreakable's 25 857 items and their waveforms — needs
//! more in one frame, and the window panicked in `create_bind_group`
//! ("range 268435456 exceeds `max_*_buffer_binding_size` limit
//! 134217728"). So the device asks for the buffer sizes this machine's
//! adapter actually has, and the defaults for everything else.

/// The limits the window's device should ask for: `wgpu_context`'s own
/// (wgpu's defaults, at most 15 inter-stage variables), with the largest
/// buffer and storage-buffer binding raised to what the adapter allows.
/// No adapter found: the defaults as they are.
#[must_use]
pub fn window_limits() -> wgpu::Limits {
    let base = wgpu::Limits {
        max_inter_stage_shader_variables: 15,
        ..wgpu::Limits::default()
    };
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        return base;
    };
    let can = adapter.limits();
    wgpu::Limits {
        max_buffer_size: base.max_buffer_size.max(can.max_buffer_size),
        max_storage_buffer_binding_size: base
            .max_storage_buffer_binding_size
            .max(can.max_storage_buffer_binding_size),
        ..base
    }
}
