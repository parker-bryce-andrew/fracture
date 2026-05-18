use super::{
    overlay_ui::{write_ui_data_to_buffer, write_ui_texture_and_handle_ui_actions},
    utility_texture::{
        OverlayImage, PositioningData, calculate_window_position, crop_frame_to_origin,
        define_frame, position_image,
    },
    utility_vertex::{
        TextureTransformed, Vertex, add_verticies_to_gpu_buffer,
        calculate_frame_transformations_for_settings,
    },
    window_cropping::{CroppedArea, if_crop_button_is_active},
    window_resizing::{if_settings_maintain_aspect_ratio, if_surface_size_changed},
};
use crate::{
    global_application_state::{
        CpuFrame, DmaFrame, FRAME_TRANSFER, FrameData, FrameLayout, LastReported,
    },
    gpu_mirror_display::{
        event_loop::INDICES,
        state::{Application, DmaStartupChecks},
        utility_texture::DmaOrCpuMemory,
        window_cropping::{InitialAbsoluteFramePosition, InitialAbsoluteWindowPosition, Size},
    },
    ui_state::{GreenScreen, VideoAspect, VideoLocation, WindowBackground, WindowBehaviour},
};
use lamco_wgpu::WgpuTexture;
use std::{sync::Arc, time::SystemTime};
use wgpu::{BindGroupLayout, Extent3d, TextureUsages, TextureView, TextureViewDescriptor};
use winit::dpi::PhysicalSize;

pub fn on_redraw(mut app: &mut Application) {
    app.systems.window.request_redraw();

    let data: Option<Arc<_>> = {
        if let Some(data) = &*FRAME_TRANSFER.lock().unwrap() {
            let data: Arc<_> = Arc::clone(&data);

            Some(data)
        } else {
            None
        }
    };

    let mut dma_copy = None;
    let imported_dma;

    if let Some(frame) = data {
        app.app_state.last_iteration.last_reported_offsets = frame.last_known_offsets;

        if let FrameData::DmaBuffers(_) = *frame.frame_data {
            dma_copy = Some(frame.frame_data.clone());
        }

        let cropped = if let Some(cropped) = &app.app_state.cropped {
            let temp: CroppedArea = cropped.clone();
            temp
        } else {
            CroppedArea {
                relative_to_window_position: InitialAbsoluteWindowPosition { x: 0, y: 0 },
                size: Size {
                    width: frame.window_dimensions.0,
                    height: frame.window_dimensions.1,
                },
                relative_to_frame_position: InitialAbsoluteFramePosition {
                    x: app.app_state.last_iteration.last_reported_offsets.0,
                    y: app.app_state.last_iteration.last_reported_offsets.1,
                },
            }
        };

        let active_button: bool = *&app.app_state.intricate_todo_refactor.crop_button_pressed;

        if_crop_button_is_active(&mut app, &active_button, &frame);

        // if VideoAspect::MaintainAspectRatio(_, WindowBehaviour::SizeMatchesMirrorAspect)
        if_settings_maintain_aspect_ratio(&app, &cropped);

        if_surface_size_changed(&mut app);

        if_frame_size_changed(&mut app, &frame);

        let verts: (Vec<Vertex>, TextureTransformed) =
            calculate_frame_transformations_for_settings(&app, &app.configuration, &cropped);

        add_verticies_to_gpu_buffer(&mut app, &verts.0);

        let active_ui_flags = app.get_active_ui_flags();

        let size = app.systems.window.inner_size();

        let overlay_text_view = write_ui_texture_and_handle_ui_actions(&mut app, size);

        // if elwt.exiting() {
        //     return;
        // }

        /*
           Sometimes, I'm writing pixels directly to a texture that will always be the same size as the window.
           Then, sometimes, I'm writing pixels to a texture that will be transformed into the size of the window.
        */

        let overlay = define_frame(&frame, &cropped);
        let positioned_frame = crop_frame_to_origin(&frame, &overlay, &cropped);

        let (sampler, ui_flags, group) = (
            app.mirror
                .render
                .shared_rendering
                .diffuse_sampler
                .take()
                .unwrap(),
            app.mirror.render.shared_rendering.ui_flags.take().unwrap(),
            app.mirror
                .render
                .shared_rendering
                .texture_bind_group_layout
                .take()
                .unwrap(),
        );

        let bindings = BindingsUsedInBindGroup {
            sampler_1: &sampler,
            ui_2: &overlay_text_view,
            ui_flags_3: &ui_flags,
            bind_group_layout: &group,
        };

        match verts.1 {
            TextureTransformed::NoTextureTransform => {
                let PhysicalSize {
                    width: phys_w,
                    height: phys_h,
                } = app.systems.window.inner_size();

                let mut loc = VideoLocation::NorthWest;

                if let VideoAspect::MaintainAspectRatio(_, WindowBehaviour::SizeSetByUser(locset)) =
                    &app.configuration.aspect_ratio
                {
                    loc = locset.clone();
                };

                // The cropped image is then positioned with the user settings
                let (x, y) = calculate_window_position(
                    &(
                        positioned_frame.dimensions_after.width as i32,
                        positioned_frame.dimensions_after.height as i32,
                    ),
                    &(phys_w as i32, phys_h as i32),
                    &loc,
                );

                let to_position = OverlayImage {
                    data: positioned_frame.data,
                    dimensions: positioned_frame.dimensions_after,
                    layout: positioned_frame.layout_after,
                };

                // The calculated coordinates above are used to crop the frame
                // once again if needed, or to adjust the width and height with
                // respect to the window/surface size
                let positioned_frame = position_image(
                    &to_position,
                    (
                        x + positioned_frame.origin.x as i32,
                        y + positioned_frame.origin.y as i32,
                    ),
                    (phys_w, phys_h),
                );

                {
                    write_ui_data_to_buffer(
                        &app.systems.wgpu.queue,
                        (&app.systems).window.inner_size(),
                        app.app_state.last_iteration.last_known_mouse_position,
                        app.user_interaction.mouse_select_start,
                        &ui_flags,
                        app.configuration.frame_transparency as f32,
                        &active_ui_flags,
                        (positioned_frame.origin.x, positioned_frame.origin.y),
                        Some((
                            positioned_frame.origin.x + positioned_frame.dimensions_after.width,
                            positioned_frame.origin.y + positioned_frame.dimensions_after.height,
                        )),
                        if let GreenScreen::Color(v) = app.configuration.green_screen.clone() {
                            Some(v)
                        } else {
                            None
                        },
                    );
                }

                create_bindings_write_texture(
                    &mut app,
                    &bindings,
                    &positioned_frame,
                    (phys_w, phys_h),
                    &frame,
                );
            }

            TextureTransformed::TextureTransformedByVerts => {
                write_ui_data_to_buffer(
                    &app.systems.wgpu.queue,
                    (&app.systems.window).inner_size(),
                    app.app_state.last_iteration.last_known_mouse_position,
                    app.user_interaction.mouse_select_start,
                    &ui_flags,
                    app.configuration.frame_transparency as f32,
                    &active_ui_flags,
                    (positioned_frame.origin.x, positioned_frame.origin.y),
                    None,
                    if let GreenScreen::Color(v) = app.configuration.green_screen.clone() {
                        Some(v)
                    } else {
                        None
                    },
                );

                create_bindings_write_texture(
                    &mut app,
                    &bindings,
                    &positioned_frame,
                    (
                        positioned_frame.dimensions_after.width,
                        positioned_frame.dimensions_after.height,
                    ),
                    &frame,
                );
            }
        }

        {
            app.mirror.render.shared_rendering.diffuse_sampler = Some(sampler);
            app.mirror.render.shared_rendering.ui_flags = Some(ui_flags);
            app.mirror.render.shared_rendering.texture_bind_group_layout = Some(group);
        }
    }

    imported_dma = match &app.systems.wgpu.bridge {
        super::event_loop::WrappedBridge::Bridged(wgpu_bridge)
        | super::event_loop::WrappedBridge::BridgedExplicitSync(wgpu_bridge) => {
            if let Some(dma) = dma_copy {
                if let FrameData::DmaBuffers(dma2) = &*dma {
                    let v = unsafe {
                        let temp = wgpu_bridge.import_dmabuf_auto(&dma2.frame_data);

                        temp
                    };

                    if let Ok(v) = v {
                        let v = v.as_single_plane().unwrap().clone();
                        Some(v)
                    } else {
                        println!("err: {:#?}", v);

                        app.app_state
                            .initialization_checks
                            .dma_startup_checks
                            .dma_error_count += 1;

                        should_panic_dma_failure_validation(
                            &mut app.app_state.initialization_checks.dma_startup_checks,
                        );

                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
        super::event_loop::WrappedBridge::Direct => None,
    };

    match app.render(imported_dma) {
        Ok(_) => {}
        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => app
            .mirror
            .render
            .resize(&mut app.systems.wgpu, app.systems.window.inner_size()),
        Err(v) => {
            println!("{v:?}");
        }
    }
}

fn padded_to_align(mut value: u32) -> u32 {
    let req = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let w_rem = value % req;
    if w_rem != 0 {
        value += req - w_rem;

        assert!(value % req == 0);
    }

    value
}

fn calculate_size(mut width: u32, mut height: u32) -> u32 {
    width = padded_to_align(width);
    height = padded_to_align(height);

    size_of::<u32>() as u32 * width * height
}

impl Application {
    fn render(&mut self, dma_data: Option<WgpuTexture>) -> Result<(), wgpu::SurfaceError> {
        let mut run_scan = false;

        while let Ok(_) = self.external.channels.gpu_frame_scan_requested.try_recv() {
            run_scan = true;
        }

        if !self
            .app_state
            .initialization_checks
            .dma_startup_checks
            .is_complete
        {
            run_scan = true;
        }

        self.mirror.render.shared_rendering.wrapping_render_count = self
            .mirror
            .render
            .shared_rendering
            .wrapping_render_count
            .wrapping_add(1);

        let settings = &self.configuration;

        let output = self.systems.wgpu.surface.get_current_texture()?;

        let pixel_perfect_inner_window_view: TextureView =
            output.texture.create_view(&wgpu::TextureViewDescriptor {
                usage: Some(wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT),
                ..Default::default()
            });

        let mut move_copy_etc_encoder =
            self.systems
                .wgpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("copy run"),
                });

        let mut mirror_output_encoder =
            self.systems
                .wgpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });

        let (cr, cg, cb, ca) = if let WindowBackground::Color(r, g, b, a) = settings.background {
            (r, g, b, a)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        {
            let mut render_pass =
                mirror_output_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &pixel_perfect_inner_window_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: cr as f64,
                                g: cg as f64,
                                b: cb as f64,
                                a: ca as f64,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

            render_pass.set_pipeline(
                &self
                    .mirror
                    .render
                    .mirror_rendering
                    .mirror_output_rendering_pipeline,
            );
            render_pass.set_bind_group(0, &self.mirror.render.shared_rendering.bindings, &[]);

            render_pass.set_vertex_buffer(
                0,
                self.mirror.render.mirror_rendering.vertex_buffer.slice(..),
            );
            render_pass.set_index_buffer(
                self.mirror.render.shared_rendering.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );

            render_pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
        }

        // with this 1 if statement, we remove a call to every pixel from the cropped region to
        // draw the user interface. The UI is only ever active when the mouse is over the screen.
        //
        // it's a good optimization because it's expected there will be lots of different versions running wasting
        // lots of resources drawing user interfaces that don't exist.
        let ui_encoder = match self.mirror.render.should_render_ui(&self) {
            true => {
                let mut ui_encoder = self.systems.wgpu.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor {
                        label: Some("Render Encoder2"),
                    },
                );
                {
                    let mut render_pass =
                        ui_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("Render Pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &pixel_perfect_inner_window_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                                depth_slice: None,
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });

                    render_pass
                        .set_pipeline(&self.mirror.render.ui_rendering.ui_rendering_pipeline);
                    render_pass.set_bind_group(
                        0,
                        &self.mirror.render.shared_rendering.bindings,
                        &[],
                    );

                    render_pass.set_vertex_buffer(
                        0,
                        self.mirror.render.ui_rendering.vertex_buffer2.slice(..),
                    );
                    render_pass.set_index_buffer(
                        self.mirror.render.shared_rendering.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );

                    render_pass.draw_indexed(0..(INDICES.len()) as u32, 0, 0..1);
                }
                Some(ui_encoder)
            }
            false => None,
        };

        let after_queue = {
            if let Some(dma) = &dma_data {
                let imported_dma = dma;
                let dma_cpu_copy_descriptor = wgpu::BufferDescriptor {
                    label: None,
                    size: calculate_size(
                        imported_dma.view().texture().width(),
                        imported_dma.view().texture().height(),
                    ) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                };

                let mut req_by_wgpu_dim = imported_dma.texture().size();
                req_by_wgpu_dim.width = padded_to_align(req_by_wgpu_dim.width);

                let crop = if let Some(v) = &self.app_state.cropped {
                    v.clone()
                } else {
                    CroppedArea {
                        relative_to_window_position: InitialAbsoluteWindowPosition { x: 0, y: 0 },
                        size: Size {
                            width: self
                                .mirror
                                .render
                                .mirror_rendering
                                .mirror_fractured_texture
                                .width(),
                            height: self
                                .mirror
                                .render
                                .mirror_rendering
                                .mirror_fractured_texture
                                .height(),
                        },
                        relative_to_frame_position: InitialAbsoluteFramePosition {
                            x: self.app_state.last_iteration.last_reported_offsets.0,
                            y: self.app_state.last_iteration.last_reported_offsets.1,
                        },
                    }
                };

                let mut from_display_origin = (
                    crop.relative_to_frame_position.x,
                    crop.relative_to_frame_position.y,
                );

                // This code is kinda bad... but I guess it works.
                {
                    let mut loc = VideoLocation::NorthWest;
                    if let VideoAspect::MaintainAspectRatio(
                        _,
                        WindowBehaviour::SizeSetByUser(locset),
                    ) = &settings.aspect_ratio
                    {
                        loc = locset.clone();
                    };

                    // if the dimensions after are less than expected
                    // then that means the surface was cut further
                    // because it needed to be positioned off screen
                    if self.app_state.last_iteration.last_fracture_dimensions.width
                        != crop.size.width
                    {
                        if self.app_state.last_iteration.last_fracture_display_origin.x == 0 {
                            let off_x = {
                                if crop.size.width
                                    >= self.app_state.last_iteration.last_fracture_dimensions.width
                                {
                                    crop.size.width
                                        - self
                                            .app_state
                                            .last_iteration
                                            .last_fracture_dimensions
                                            .width
                                } else {
                                    0
                                }
                            };
                            match loc {
                                VideoLocation::West
                                | VideoLocation::NorthWest
                                | VideoLocation::SouthWest => {}

                                VideoLocation::Center
                                | VideoLocation::North
                                | VideoLocation::South => from_display_origin.0 += off_x / 2,

                                VideoLocation::NorthEast
                                | VideoLocation::East
                                | VideoLocation::SouthEast => from_display_origin.0 += off_x,
                            }
                        }
                    }

                    if self
                        .app_state
                        .last_iteration
                        .last_fracture_dimensions
                        .height
                        != crop.size.height
                    {
                        if self.app_state.last_iteration.last_fracture_display_origin.y == 0 {
                            let off_y = if crop.size.height
                                > self
                                    .app_state
                                    .last_iteration
                                    .last_fracture_dimensions
                                    .height
                            {
                                crop.size.height
                                    - self
                                        .app_state
                                        .last_iteration
                                        .last_fracture_dimensions
                                        .height
                            } else {
                                0
                            };

                            match loc {
                                VideoLocation::NorthWest
                                | VideoLocation::North
                                | VideoLocation::NorthEast => {}
                                VideoLocation::Center
                                | VideoLocation::East
                                | VideoLocation::West => {
                                    from_display_origin.1 += off_y / 2;
                                }

                                VideoLocation::SouthWest
                                | VideoLocation::South
                                | VideoLocation::SouthEast => {
                                    from_display_origin.1 += off_y;
                                }
                            }
                        }
                    }
                }

                move_copy_etc_encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        aspect: wgpu::TextureAspect::All,
                        texture: &imported_dma.texture(),
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: from_display_origin.0,
                            y: from_display_origin.1,
                            z: 0,
                        },
                    },
                    wgpu::TexelCopyTextureInfo {
                        aspect: wgpu::TextureAspect::All,
                        texture: &self.mirror.render.mirror_rendering.mirror_fractured_texture,
                        mip_level: 0,
                        origin: self.app_state.last_iteration.last_fracture_display_origin,
                    },
                    Extent3d {
                        width: self.app_state.last_iteration.last_fracture_dimensions.width,
                        height: self
                            .app_state
                            .last_iteration
                            .last_fracture_dimensions
                            .height,
                        ..Default::default()
                    },
                );

                let cpu_copy_dma_buf_data: Option<wgpu::Buffer> = match run_scan {
                    true => {
                        let cpu_copy_dma_buf_data = self
                            .systems
                            .wgpu
                            .device
                            .create_buffer(&dma_cpu_copy_descriptor);

                        // The padded buffer is only needed to copy to a cpu buffer.
                        let padded_texture_buffer =
                            self.systems
                                .wgpu
                                .device
                                .create_texture(&wgpu::TextureDescriptor {
                                    label: None,
                                    size: req_by_wgpu_dim,
                                    mip_level_count: imported_dma.texture().mip_level_count(),
                                    sample_count: imported_dma.texture().sample_count(),
                                    dimension: imported_dma.texture().dimension(),
                                    format: imported_dma.texture().format(),
                                    usage: TextureUsages::COPY_DST | TextureUsages::COPY_SRC,
                                    view_formats: &vec![imported_dma.texture().format()],
                                });

                        move_copy_etc_encoder.copy_texture_to_texture(
                            wgpu::TexelCopyTextureInfo {
                                aspect: wgpu::TextureAspect::All,
                                texture: &imported_dma.texture(),
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                            },
                            wgpu::TexelCopyTextureInfo {
                                aspect: wgpu::TextureAspect::All,
                                texture: &padded_texture_buffer,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                            },
                            Extent3d {
                                width: imported_dma.texture().width(),
                                height: imported_dma.texture().height(),
                                ..Default::default()
                            },
                        );

                        move_copy_etc_encoder.copy_texture_to_buffer(
                            wgpu::TexelCopyTextureInfo {
                                aspect: wgpu::TextureAspect::All,
                                texture: &padded_texture_buffer,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                            },
                            wgpu::TexelCopyBufferInfo {
                                buffer: &cpu_copy_dma_buf_data,
                                layout: wgpu::TexelCopyBufferLayout {
                                    offset: 0,
                                    bytes_per_row: Some(
                                        padded_texture_buffer.width() * (size_of::<u32>() as u32),
                                    ),
                                    rows_per_image: Some(padded_texture_buffer.height()),
                                },
                            },
                            padded_texture_buffer.size(),
                        );

                        Some(cpu_copy_dma_buf_data)
                    }
                    false => None,
                };

                let mut imported_dim = imported_dma.texture().size();
                imported_dim.width = padded_to_align(imported_dim.width);

                let dev = self.systems.wgpu.device.clone();

                let after_queue = move |state| {
                    let state: &mut Self = state;

                    if let Some(cpu_copy_dma_buf_data) = cpu_copy_dma_buf_data {
                        let output_buffer = cpu_copy_dma_buf_data;
                        let cpu_data_buffer_slice = output_buffer.slice(..);

                        let rt = state.systems.async_rt.as_ref().unwrap();
                        let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
                        cpu_data_buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                            tx.send(result).unwrap();
                        });

                        dev.poll(wgpu::PollType::wait_indefinitely()).unwrap();

                        rt.block_on(async { rx.receive().await }).unwrap().unwrap();

                        let cpu_data = {
                            let data = cpu_data_buffer_slice.get_mapped_range();

                            std::vec::Vec::from(&data[..])
                        };

                        let (w, h) = (imported_dim.width, imported_dim.height);

                        {
                            let mut temp = FRAME_TRANSFER.lock().unwrap();

                            let inner: Option<Arc<_>> = temp.clone();

                            if inner.is_none() {
                                return;
                            }

                            let inner: Arc<_> = inner.unwrap();
                            let inner: &LastReported = &inner;
                            let mut last: LastReported = inner.clone();

                            let mut dma: DmaFrame = match &*last.frame_data {
                                FrameData::CpuData(_) => {
                                    unreachable!("The CPU frames should never have a DMA buffer")
                                }
                                FrameData::DmaBuffers(dma_frame) => dma_frame.clone(),
                            };

                            let cpu = CpuFrame {
                                frame_data: cpu_data,
                                layout: FrameLayout {
                                    width: w,
                                    height: h,
                                    bytes_per_pixel: 4,
                                },
                                scan_time: SystemTime::now(),
                            };

                            let cpu_data = Arc::new(cpu);

                            if !state
                                .app_state
                                .initialization_checks
                                .dma_startup_checks
                                .is_complete
                            {
                                state
                                    .app_state
                                    .initialization_checks
                                    .dma_startup_checks
                                    .frames_checked += 1;

                                let check = cpu_data.clone();
                                let frames_without_data = state
                                    .app_state
                                    .initialization_checks
                                    .dma_startup_checks
                                    .frames_without_data
                                    .clone();
                                let frames_with_data = state
                                    .app_state
                                    .initialization_checks
                                    .dma_startup_checks
                                    .frames_with_data
                                    .clone();

                                std::thread::spawn(move || {
                                    let data = &check.frame_data;

                                    let mut pass = false;

                                    'scan: for v in data {
                                        if *v > 0 {
                                            pass = true;

                                            break 'scan;
                                        }
                                    }

                                    if pass {
                                        *frames_with_data.lock().unwrap() += 1;
                                    } else {
                                        *frames_without_data.lock().unwrap() += 1;
                                    }
                                });

                                should_panic_dma_failure_validation(
                                    &mut state.app_state.initialization_checks.dma_startup_checks,
                                );
                            }

                            dma.saved_cpu_frame = Some(cpu_data);
                            last.frame_data = Arc::new(FrameData::DmaBuffers(dma));

                            *temp = Some(Arc::new(last));
                        }

                        output_buffer.unmap();

                        state.app_state.initialization_checks.first_dma_sent = true;
                    }
                };

                Some(after_queue)
            } else {
                None
            }
        };

        {
            self.systems
                .wgpu
                .queue
                .submit(std::iter::once(move_copy_etc_encoder.finish()));
            self.systems
                .wgpu
                .queue
                .submit(std::iter::once(mirror_output_encoder.finish()));

            if let Some(ui_encoder) = ui_encoder {
                self.systems
                    .wgpu
                    .queue
                    .submit(std::iter::once(ui_encoder.finish()));
            }
        }

        if let Some(after) = after_queue {
            after(self);
        }

        output.present();

        Ok(())
    }
}

pub fn should_panic_dma_failure_validation(dma_startup: &mut DmaStartupChecks) {
    if !dma_startup.is_complete {
        if *dma_startup.frames_with_data.lock().unwrap() > 0 {
            dma_startup.is_complete = true;
        } else {
            let mut should_fail = false;

            if dma_startup.dma_error_count > dma_startup.fail_at {
                should_fail = true;
            }

            if dma_startup.frames_checked > dma_startup.fail_at {
                if *dma_startup.frames_without_data.lock().unwrap() > dma_startup.fail_at {
                    should_fail = true;
                }
            }

            if should_fail {
                if let Err(_) = std::env::var("ASSUME_DMA_IS_GOOD") {
                    dma_startup.is_complete = true;
                    dma_startup.is_fail = true;

                    println!("FAILED TO FIND A SUCCESSFUL DMA BUFFER.");
                    println!("");
                    println!("{:#?}", dma_startup);
                    println!("");
                    println!("To suppress this failure, start with ASSUME_DMA_IS_GOOD");

                    panic!(
                        "Failed tests for using DMA buffers. Attempting to restart with CPU buffers.",
                    )
                } else {
                    dma_startup.is_complete = true;
                    dma_startup.is_fail = false;
                }
            }
        }
    }
}

pub struct BindingsUsedInBindGroup<'a> {
    pub bind_group_layout: &'a BindGroupLayout,
    // pub video_display_0: Option<&'a TextureView>,
    pub sampler_1: &'a wgpu::Sampler,
    pub ui_2: &'a TextureView,
    pub ui_flags_3: &'a wgpu::Buffer,
}

fn create_bindings_write_texture(
    app: &mut Application,
    bindings: &BindingsUsedInBindGroup,
    positioned_frame: &PositioningData<'_>,
    (width, height): (u32, u32),
    last: &LastReported,
) {
    let tex = app
        .systems
        .wgpu
        .device
        .create_texture(&wgpu::TextureDescriptor {
            size: Extent3d {
                width: width,
                height: height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: app.mirror.render.shared_rendering.used_video_format.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("diffuse_texture"),
            view_formats: &[],
        });

    {
        let BindingsUsedInBindGroup {
            bind_group_layout,
            sampler_1,
            ui_2,
            ui_flags_3,
        } = bindings;

        let diffuse_bind_group =
            app.systems
                .wgpu
                .device
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    layout: &bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                &tex.create_view(&TextureViewDescriptor::default()),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler_1),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&ui_2),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Buffer(
                                ui_flags_3.as_entire_buffer_binding(),
                            ),
                        },
                    ],
                    label: Some("diffuse_bind_group"),
                });

        app.mirror.render.shared_rendering.bindings = diffuse_bind_group;

        app.app_state.last_iteration.last_fracture_display_origin = positioned_frame.origin.clone();
        app.app_state.last_iteration.last_fracture_dimensions =
            positioned_frame.dimensions_after.clone();

        let data = match &positioned_frame.data {
            DmaOrCpuMemory::Dma => None,
            DmaOrCpuMemory::Cpu(items) => Some(items),
        };

        if let FrameData::CpuData(_) = &*last.frame_data {
            if data.is_none() {
                return;
            }

            app.systems.wgpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: 0,
                    origin: positioned_frame.origin,
                    aspect: wgpu::TextureAspect::All,
                },
                data.unwrap(),
                positioned_frame.layout_after,
                positioned_frame.dimensions_after,
            );
        }
    }

    app.mirror.render.mirror_rendering.mirror_fractured_texture = tex;
}

fn if_frame_size_changed(app: &mut Application, frame: &Arc<LastReported>) {
    let last_frame_size = &mut app.app_state.last_iteration.last_frame_size;

    if *last_frame_size != frame.window_dimensions {
        *last_frame_size = frame.window_dimensions;
        app.mirror
            .render
            .resize(&mut app.systems.wgpu, app.systems.window.inner_size());
    }
}
