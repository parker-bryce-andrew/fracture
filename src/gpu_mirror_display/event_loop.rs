use crate::application_channel_creator::GpuChannelSide;
use crate::global_application_state::{AVAILABLE_PRESETS, FPS_TRACKING, SAFE_MODE, load_profiles};
use crate::gpu_mirror_display::defaults::{APPLICATION_NAME, PRESENT_PREFERENCES};
use crate::gpu_mirror_display::input::events_mouse::ResizeInteractionsState;
use crate::gpu_mirror_display::input::on_input_events;
use crate::gpu_mirror_display::input::utility_mouse::remove_expired_mouse_events;
use crate::gpu_mirror_display::overlay_ui::write_ui_data_to_buffer;
use crate::gpu_mirror_display::pipeline_definitions::{
    define_primary_pipeline, define_primary_sampler, select_caps_with_preferences,
};
use crate::gpu_mirror_display::postprocessing_shaders::{
    define_postprocessing_mirror_shader, if_shader_compilation_requested,
};
use crate::gpu_mirror_display::render::on_redraw;
use crate::gpu_mirror_display::state::{
    AppState, AppStatistics, AppSystems, Application, COMPLETE_RESIZE_ON_NEW_SETTINGS_AFTER,
    DmaStartupChecks, EnumeratedState, ExternalControl, FpsTracker, FpsTrackerOrigin, InitState,
    IntricateState, Mirror, MirrorRenderer, MirrorRendering, PipewireController, PreviousIteration,
    SettingsGtk, SharedRender, UiRendering, UserInteractionState, WgpuContainer,
};
use crate::gpu_mirror_display::utility_texture::DmaOrCpuMemory;
use crate::gpu_mirror_display::utility_vertex::{VERTICES, Vertex};
use crate::gpu_mirror_display::window_cropping::start_crop_selection;
use crate::gpu_mirror_display::{binary_images, shutdown};
use crate::ui_state::{
    AppConfiguration, DEFAULT_MAGNIFY_FILTER, DEFAULT_MINIFY_FILTER, TitleBarDisplay,
    WindowInteractions,
};
use lamco_wgpu::SupportedFormat;
use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;
use std::{env, mem};
use wgpu::util::DeviceExt;
use wgpu::{BufferDescriptor, BufferUsages, Extent3d, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::window::{Window, WindowAttributes, WindowId};
use winit::{error::EventLoopError, event_loop::EventLoop};

pub enum WrappedBridge {
    Bridged(lamco_wgpu::WgpuBridge),
    BridgedExplicitSync(lamco_wgpu::WgpuBridge),
    Direct,
}

#[derive(Clone)]
pub struct WebGpuReport {
    pub formats: Option<Vec<SupportedFormat>>,
    pub using_bridge: bool,
}
pub const INDICES: &[u16] = &[2, 3, 1, 2, 1, 0];
struct WinitHandler {
    app: Option<Application>,
    initialization_only: Option<GpuChannelSide>,
    about_to_wait_count: u32,
}

impl ApplicationHandler<()> for WinitHandler {
    fn user_event(&mut self, _: &ActiveEventLoop, _: ()) {}

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.app.is_some() {
            return;
        }

        let channels: GpuChannelSide = self.initialization_only.take().unwrap();

        if let Ok(received_stream) = channels.stream_start_check_mirror_gpu.recv() {
            if !received_stream {
                println!("failed to select stream");

                event_loop.exit();
                return;
            }
        } else {
            return;
        }

        let bridge = {
            let mut bridge: WrappedBridge = WrappedBridge::Direct;

            if let Err(_) = std::env::var(SAFE_MODE) {
                match lamco_wgpu::bridge::WgpuBridge::new_with_explicit_sync() {
                    Ok(sync_bridge) => {
                        bridge = WrappedBridge::BridgedExplicitSync(sync_bridge);
                    }
                    Err(_) => match lamco_wgpu::bridge::WgpuBridge::new() {
                        Ok(no_sync) => bridge = WrappedBridge::Bridged(no_sync),
                        _ => {}
                    },
                }
            }

            bridge
        };

        match &bridge {
            WrappedBridge::Bridged(bridge) | WrappedBridge::BridgedExplicitSync(bridge) => {
                let report: Vec<SupportedFormat> = bridge
                    .supported_formats()
                    .iter()
                    .map(|v| v.clone())
                    .collect();

                let report = WebGpuReport {
                    formats: Some(report),
                    using_bridge: true,
                };

                channels.webgpu_drm_report.send(report).unwrap();

                println!("Sync: {:?}", bridge.sync_capabilities())
            }
            WrappedBridge::Direct => {
                let report = WebGpuReport {
                    formats: None,
                    using_bridge: false,
                };

                channels.webgpu_drm_report.send(report).unwrap();
                println!("Sync: Direct Wgpu, no sync available")
            }
        }

        // before starting, wait for the video format
        let video_format = channels.predicted_frame_fmt_receiver.recv().unwrap();

        let at = {
            let size = LogicalSize::new(video_format.width as f64, video_format.height as f64);

            let at = WindowAttributes::default()
                .with_name(APPLICATION_NAME, APPLICATION_NAME)
                .with_title(APPLICATION_NAME)
                .with_transparent(true)
                .with_inner_size(size)
                .with_resizable(true);

            at
        };

        let window = Some(Arc::new(event_loop.create_window(at).unwrap()));

        let overlay_dimensions = binary_images::ICON_SELECT_SCREEN_AREA.dimensions;

        let instance = match &bridge {
            WrappedBridge::Bridged(bridge) | WrappedBridge::BridgedExplicitSync(bridge) => {
                bridge.instance.clone()
            }
            WrappedBridge::Direct => wgpu::Instance::new(&wgpu::InstanceDescriptor::default()),
        };

        let temp: &Arc<Window> = &window.as_ref().unwrap();
        let temp: Arc<Window> = temp.clone();

        let surface: Surface<'static> = instance.create_surface(temp).unwrap();

        // todo: Maybe add the ability to switch adapters as a method for GPU selection
        // let list: Vec<wgpu::Adapter> = instance.enumerate_adapters(wgpu::Backends::all());
        // let list2: Vec<String> = list.iter().map(|v| v.get_info().name).collect();
        // println!("{list2:#?}");

        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        let adapter = rt.block_on(async {
            match &bridge {
                WrappedBridge::Bridged(wgpu_bridge)
                | WrappedBridge::BridgedExplicitSync(wgpu_bridge) => wgpu_bridge.adapter().clone(),
                WrappedBridge::Direct => {
                    let adapter = instance
                        .request_adapter(&wgpu::RequestAdapterOptions {
                            compatible_surface: Some(&surface),
                            ..Default::default()
                        })
                        .await;

                    match adapter {
                        Ok(adapter) => adapter,
                        Err(_) => instance
                            .request_adapter(&wgpu::RequestAdapterOptions {
                                compatible_surface: Some(&surface),
                                force_fallback_adapter: true,
                                ..Default::default()
                            })
                            .await
                            .unwrap(),
                    }
                }
            }
        });

        let (device, queue) = rt.block_on(async {
            match &bridge {
                WrappedBridge::Bridged(wgpu_bridge)
                | WrappedBridge::BridgedExplicitSync(wgpu_bridge) => {
                    (wgpu_bridge.device().clone(), wgpu_bridge.queue().clone())
                }
                WrappedBridge::Direct => adapter
                    .request_device(&wgpu::DeviceDescriptor::default())
                    .await
                    .unwrap(),
            }
        });

        #[cfg(not(debug_assertions))]
        {
            // There are slight validation errors, as an example, a known 1 is that
            // if the streaming window is resized above the size that Gnome initiated for the screen
            // recorder, then wgpu queue validation will panic with
            //
            /*
               wgpu error: Validation Error

               Caused by:
               In Queue::write_texture
                   Number of bytes per row is less than the number of bytes in a complete row
            */
            //
            // When an error scope is added, adding a filter prevents wgpu from causing a panic for
            // the selected error type. It's left out for debug builds so that it's easily known
            // that there's something THAT IS WRONG, but added for release builds because
            // it should provide better application stability without significantly impacting expected
            // functionality.

            let uncap_err_handler: std::sync::Arc<Box<dyn wgpu::UncapturedErrorHandler>> =
                std::sync::Arc::new(Box::new(|err| {
                    println!("wgpu err: {err:#?}");
                }));

            device.on_uncaptured_error(uncap_err_handler);

            // device.push_error_scope(wgpu::ErrorFilter::Validation);
        }

        let surface_caps = surface.get_capabilities(&adapter);

        let ordered_presets: Vec<_> = PRESENT_PREFERENCES
            .iter()
            .filter(|pre| *&surface_caps.present_modes.contains(pre))
            .map(|v| *v)
            .collect();

        {
            *AVAILABLE_PRESETS.lock().unwrap() = ordered_presets.clone();
        }

        let selected_surface_capabilities =
            select_caps_with_preferences(surface_caps, video_format.format);
        // surface_caps.
        /*         let surface_caps = surface.get_capabilities(&adapter);
         */
        /*     let surface_format = surface_caps
                   .formats
                   .iter()
                   .find(|f| f.is_srgb())
                   .copied()
                   .unwrap_or(surface_caps.formats[0]);
        */
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: selected_surface_capabilities.texture, //surface_format,
            width: video_format.width,
            height: video_format.height,
            // Mailbox seems to have the best performance, but it also significantly raises GPU
            // utilization. AutoVsync performs OK, but the playback is not smooth. I need to spend
            // more time attempting to optimize the pipeline.
            present_mode: selected_surface_capabilities.present,

            alpha_mode: selected_surface_capabilities.alpha, //surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        let size = PhysicalSize::new(video_format.width as u32, video_format.height as u32);

        let shader = rt
            .block_on(define_postprocessing_mirror_shader(&device, None))
            .unwrap();

        // println!("{:}", shader.extract_postprocessor());

        let shader = shader.device_module;

        let shader2 = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/ui.wgsl").into()),
        });

        surface.configure(&device, &config);

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let vertex_buffer2 = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        surface.configure(&device, &config);

        let texture_size = wgpu::Extent3d {
            width: 500,
            height: 500,
            depth_or_array_layers: 1,
        };

        let overlay_text = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,

            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,

            format: wgpu::TextureFormat::Bgra8Unorm,

            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("diffuse_texture"),

            view_formats: &[],
        });

        let diffuse_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: video_format.width,
                height: video_format.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,

            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,

            format: video_format.format,

            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("diffuse_texture"),

            view_formats: &[],
        });

        let overlay_text_view = overlay_text.create_view(&wgpu::TextureViewDescriptor::default());

        let diffuse_texture_view =
            diffuse_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let diffuse_sampler: wgpu::Sampler =
            define_primary_sampler(&device, DEFAULT_MAGNIFY_FILTER, DEFAULT_MINIFY_FILTER);

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,

                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZero::new(68u64),
                        },
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
            });

        let ui_flags = device.create_buffer(&BufferDescriptor {
            label: None,
            size: 68,
            usage: BufferUsages::COPY_DST | BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });

        let diffuse_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diffuse_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&overlay_text_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(ui_flags.as_entire_buffer_binding()),
                },
            ],
            label: Some("diffuse_bind_group"),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&texture_bind_group_layout],
                // push_constant_ranges: &[],
                immediate_size: 0,
            });

        let render_pipeline =
            define_primary_pipeline(&device, &shader, &render_pipeline_layout, &config.format);

        let render_pipeline2 = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader2,
                entry_point: Some("vs_main"), // 1.
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x3,
                        },
                        wgpu::VertexAttribute {
                            offset: mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader2,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),

            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },

            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            // multiview: None,
            cache: None,
            multiview_mask: None,
        });

        let overlay_size = Extent3d {
            width: overlay_dimensions.width,
            height: overlay_dimensions.height,
            depth_or_array_layers: 1,
        };

        write_ui_data_to_buffer(
            &queue,
            window.as_ref().unwrap().inner_size(),
            (0, 0),
            (0, 0),
            &ui_flags,
            0.0,
            &vec![],
            (0, 0),
            None,
            None,
        );

        let DmaOrCpuMemory::Cpu(icon_select_screen_area_data) =
            &binary_images::ICON_SELECT_SCREEN_AREA.data
        else {
            unreachable!("Not possible");
        };

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &overlay_text,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            icon_select_screen_area_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * 500),
                rows_per_image: Some(500),
            },
            overlay_size,
        );

        let sel = selected_surface_capabilities.present.clone();

        let profiles = load_profiles().unwrap_or(Default::default());
        let mut saved = profiles.user_default();

        // The user can have pass through interactions in their profiles, but the application
        // always converts the startup to interactable to prevent the user from creating
        // a situation where they are stuck.
        saved.config.window_interactions = Some(WindowInteractions::Interactable);

        let mut app = Application {
            app_state: AppState {
                cropped: None,
                current_activity: EnumeratedState::WaitingForSelection,
                initialization_checks: InitState {
                    dma_startup_checks: DmaStartupChecks {
                        is_complete: false,
                        is_fail: false,
                        frames_checked: 0,
                        frames_without_data: std::sync::Arc::new(std::sync::Mutex::new(0)),
                        frames_with_data: std::sync::Arc::new(std::sync::Mutex::new(0)),
                        fail_at: 30,
                        dma_error_count: 0,
                    },
                    first_dma_sent: false,
                    track_fps: match env::var(FPS_TRACKING) {
                        Ok(_) => true,
                        Err(_) => false,
                    },
                },
                last_iteration: PreviousIteration {
                    last_fracture_display_origin: Default::default(),
                    last_fracture_dimensions: Default::default(),
                    last_reported_offsets: (0, 0),
                    last_surface_size: window.as_ref().unwrap().inner_size(),
                    last_frame_size: (0, 0),
                    last_known_mouse_position: (0, 0),
                },
                intricate_todo_refactor: IntricateState {
                    crop_button_pressed: true,
                    in_crop_selection: true,
                    keep_borders: false,
                    resize_countdown_from_new_settings: 0,
                    resize_countdown_started: true,
                    should_shutdown: false,
                    active_present: sel,
                    new_settings: true,
                },
            },
            user_interaction: UserInteractionState {
                mouse_clicks: vec![],
                mouse_downs: vec![],
                mouse_over_screen: false,
                mouse_is_down: false,
                mouse_select_start: (0, 0),
                mouse_resize_state: ResizeInteractionsState::None,
            },
            mirror: Mirror {
                render: MirrorRenderer {
                    ui_rendering: UiRendering {
                        ui_rendering_pipeline: render_pipeline2.clone(),
                        vertex_buffer2: vertex_buffer2.clone(),
                    },
                    mirror_rendering: MirrorRendering {
                        mirror_output_rendering_pipeline: render_pipeline.clone(),
                        vertex_buffer: vertex_buffer,
                        mirror_fractured_texture: device.create_texture(&wgpu::TextureDescriptor {
                            size: Extent3d {
                                width: size.width,
                                height: size.height,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: video_format.format,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_DST,
                            label: Some("mirror_placeholder"),
                            view_formats: &[],
                        }),
                        pipeline_layout: Some(render_pipeline_layout),
                    },
                    shared_rendering: SharedRender {
                        index_buffer: index_buffer,
                        diffuse_sampler: Some(diffuse_sampler),
                        bindings: diffuse_bind_group,
                        ui_flags: Some(ui_flags),
                        used_video_format: video_format.clone(),
                        texture_bind_group_layout: Some(texture_bind_group_layout),
                        wrapping_render_count: 0,
                        available_presents: ordered_presets,
                        default_selected_capabilities: selected_surface_capabilities,
                    },
                },
            },
            configuration: AppConfiguration {
                active: saved.config.clone().into(),
                saved: saved.config,
                profiles: profiles,
            },
            systems: AppSystems {
                wgpu: WgpuContainer {
                    bridge: bridge,
                    device: device,
                    queue: queue,
                    config: config,
                    surface: surface,
                },
                window: window.as_ref().unwrap().clone(),
                async_rt: Some(rt),
            },
            external: ExternalControl {
                settings_ui: SettingsGtk,
                pipewire: PipewireController,
                channels: Arc::new(channels),
            },
            metrics: AppStatistics {
                fps_tracking: FpsTracker::new(
                    Some(FpsTrackerOrigin::WebGpu),
                    Some(vec![
                        Duration::from_secs(1),
                        Duration::from_secs(15),
                        Duration::from_secs(60),
                        Duration::from_secs(60 * 5),
                        Duration::from_secs(60 * 15),
                        Duration::from_secs(60 * 60),
                    ]),
                ),
            },
        };

        app.external
            .channels
            .gpu_sender_request
            .send(app.configuration.active.clone())
            .unwrap();

        app.mirror
            .render
            .resize(&mut app.systems.wgpu, window.as_ref().unwrap().inner_size());

        start_crop_selection(&mut app);
        on_redraw(&mut app);

        self.app = Some(app);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if !self.app.is_some() {
            return;
        }

        let mut app = self.app.take().unwrap();

        // [1] state.resize is expensive, but needs to happen or the image will become distorted.
        //
        // To manage, it waits until a set number (60 frames) until calling the funciton to resize.
        // This is because it's expected that most of the time, there won't be new settings, so
        // the image can become temporarily distorted without worrying about it.
        if app
            .app_state
            .intricate_todo_refactor
            .resize_countdown_started
        {
            app.app_state
                .intricate_todo_refactor
                .resize_countdown_from_new_settings -= 1;

            if app
                .app_state
                .intricate_todo_refactor
                .resize_countdown_from_new_settings
                <= 0
            {
                app.mirror
                    .render
                    .resize(&mut app.systems.wgpu, app.systems.window.inner_size());
                app.app_state
                    .intricate_todo_refactor
                    .resize_countdown_started = false;
            }
        }

        while let Ok(new) = app.external.channels.new_settings_receiver.try_recv() {
            // This is expensive, but needs to happen or the image will become distorted. [2]
            if !app
                .app_state
                .intricate_todo_refactor
                .resize_countdown_started
            {
                app.app_state
                    .intricate_todo_refactor
                    .resize_countdown_from_new_settings = COMPLETE_RESIZE_ON_NEW_SETTINGS_AFTER;

                app.app_state
                    .intricate_todo_refactor
                    .resize_countdown_started = true;
            }

            app.configuration.active = new;
            app.app_state.intricate_todo_refactor.new_settings = true;

            let mut should_update = false;

            if app.configuration.active.reload_profiles {
                app.configuration.profiles = load_profiles().unwrap_or(Default::default());

                app.configuration.active.reload_profiles = false;

                should_update = true;
            }

            if app.configuration.active.should_define_new_primary_sampler {
                app.mirror.render.shared_rendering.diffuse_sampler = Some(define_primary_sampler(
                    &app.systems.wgpu.device,
                    app.configuration.active.magnify_filter,
                    app.configuration.active.minify_filter,
                ));

                app.configuration.active.should_define_new_primary_sampler = false;

                should_update = true;
            }

            if app.configuration.active.should_define_new_preset {
                let pre = app.configuration.active.preset.clone();

                let cfg = match app
                    .mirror
                    .render
                    .shared_rendering
                    .available_presents
                    .contains(&pre)
                {
                    true => {
                        let PhysicalSize {
                            width: w,
                            height: h,
                        } = app.systems.window.inner_size();

                        let config = wgpu::SurfaceConfiguration {
                            usage: wgpu::TextureUsages::COPY_SRC
                                | wgpu::TextureUsages::RENDER_ATTACHMENT,
                            format: app
                                .mirror
                                .render
                                .shared_rendering
                                .default_selected_capabilities
                                .texture,
                            width: w,
                            height: h,
                            present_mode: pre,
                            alpha_mode: app
                                .mirror
                                .render
                                .shared_rendering
                                .default_selected_capabilities
                                .alpha,
                            view_formats: vec![],
                            desired_maximum_frame_latency: 2,
                        };

                        app.app_state.intricate_todo_refactor.active_present = pre.clone();
                        app.systems
                            .wgpu
                            .surface
                            .configure(&app.systems.wgpu.device, &config);

                        config
                    }
                    false => {
                        app.configuration.active.preset = app
                            .mirror
                            .render
                            .shared_rendering
                            .default_selected_capabilities
                            .present
                            .clone();

                        let PhysicalSize {
                            width: w,
                            height: h,
                        } = app.systems.window.inner_size();

                        let config = wgpu::SurfaceConfiguration {
                            usage: wgpu::TextureUsages::COPY_SRC
                                | wgpu::TextureUsages::RENDER_ATTACHMENT,
                            format: app
                                .mirror
                                .render
                                .shared_rendering
                                .default_selected_capabilities
                                .texture,
                            width: w,
                            height: h,
                            present_mode: app
                                .mirror
                                .render
                                .shared_rendering
                                .default_selected_capabilities
                                .present,
                            alpha_mode: app
                                .mirror
                                .render
                                .shared_rendering
                                .default_selected_capabilities
                                .alpha,
                            view_formats: vec![],
                            desired_maximum_frame_latency: 2,
                        };

                        app.app_state.intricate_todo_refactor.active_present = app
                            .mirror
                            .render
                            .shared_rendering
                            .default_selected_capabilities
                            .present
                            .clone();

                        app.systems
                            .wgpu
                            .surface
                            .configure(&app.systems.wgpu.device, &config);

                        config
                    }
                };

                app.systems.wgpu.config = cfg;

                should_update = true;

                app.configuration.active.should_define_new_preset = false;
            }

            if should_update {
                app.external
                    .channels
                    .gpu_sender_request
                    .send(app.configuration.active.clone())
                    .unwrap();
            }
        }
        match event {
            WindowEvent::RedrawRequested => {
                on_redraw(&mut app);

                if app.app_state.intricate_todo_refactor.should_shutdown {
                    let _ = shutdown::shutdown(event_loop, app);

                    return;
                }
            }
            WindowEvent::CloseRequested => {
                let _ = shutdown::shutdown(event_loop, app);

                return;
            }
            WindowEvent::Focused(false) => {
                {
                    // So, after cropping, the application started resizing when losing
                    // focus to the last dragged window size. When searching on Google, the
                    // response provided was
                    //
                    //      Windows resizing or shrinking upon losing focus in Wayland is
                    //      often caused by Client-Side Decorations (CSD) acting
                    //      incorrectly, where the window shrinks to match reduced shadows wh...
                    //
                    // I really don't know where I went wrong, if I even made a mistake... So the hack
                    // to fix this issue is to resize the window when the focus is lost to make
                    // sure it matches the last known size the application detected. (As something
                    // is resizing the window when the focus is lost)

                    app.systems
                        .window
                        .request_inner_size(app.app_state.last_iteration.last_surface_size)
                        .unwrap();

                    // There's a focus check for mouse downs
                    on_input_events(&mut app, &event);
                }
            }

            _ => on_input_events(&mut app, &event),
        }

        if app.app_state.intricate_todo_refactor.new_settings {
            if let TitleBarDisplay::TitleBarVisible = &app.configuration.active.display_title {
                app.systems.window.set_decorations(true);
            } else {
                app.systems.window.set_decorations(false);
            }

            match &app.configuration.active.window_interactions {
                crate::ui_state::WindowInteractions::Interactable => {
                    let _ = app.systems.window.set_cursor_hittest(true);
                }
                crate::ui_state::WindowInteractions::PassThrough => {
                    let _ = app.systems.window.set_cursor_hittest(false);
                }
            }

            let rt = app.systems.async_rt.take().unwrap();
            let render_pipeline_layout = app
                .mirror
                .render
                .mirror_rendering
                .pipeline_layout
                .take()
                .unwrap();

            if_shader_compilation_requested(&mut app, &rt, &render_pipeline_layout);

            app.systems.async_rt = Some(rt);
            app.mirror.render.mirror_rendering.pipeline_layout = Some(render_pipeline_layout);
        }

        remove_expired_mouse_events(&mut app);

        app.app_state.intricate_todo_refactor.new_settings = false;

        self.app = Some(app);
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, _: DeviceEvent) {}

    fn about_to_wait(&mut self, ev: &ActiveEventLoop) {
        match &self.app {
            Some(w) => {
                w.systems.window.request_redraw();
            }
            None => {
                if ev.exiting() {
                    println!("shutting down");
                    return;
                } else {
                    self.about_to_wait_count += 1;

                    if self.about_to_wait_count > 100 {
                        panic!(
                            "shutting down because the wait count was too high without a window"
                        );
                    }
                }
            }
        }
    }
}

pub fn run_mirror_video_output_ui(channels: GpuChannelSide) -> Result<(), EventLoopError> {
    let event_loop = EventLoop::new().unwrap();

    let mut handler = WinitHandler {
        app: None,
        initialization_only: Some(channels),
        about_to_wait_count: 0,
    };

    event_loop.run_app(&mut handler)
}
