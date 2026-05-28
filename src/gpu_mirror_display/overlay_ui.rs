use super::{
    START_TIME, binary_images,
    input::utility_mouse::{first_in_range, found_remove_mouse_click, mouse_in_img_bounds},
    shutdown,
    utility_texture::write_image_to_texture,
    window_cropping::start_crop_selection,
};
use crate::{
    global_application_state::load_profiles,
    gpu_mirror_display::state::Application,
    ui_state::{CreateUiState, RemoveColors, TitleBarDisplay, UiState},
};
use wgpu::{Extent3d, Queue, TextureDescriptor, TextureView, TextureViewDescriptor};
use winit::dpi::PhysicalSize;

/// The name is misleading, this writes the UI textures, but it also handles clicking on the UI
/// because the logic is easier to follow when both of these are done together. An improved UI system
/// would likely define types like buttons and then the rendering system would handle rendering those buttons,
/// but that's a lot more, and this application hopefully won't need a complex UI system.
pub fn write_ui_texture_and_handle_ui_actions(
    app: &mut Application,
    surface_size: PhysicalSize<u32>,
) -> TextureView {
    let ui_settings: &UiState = &&app.configuration.active.clone();
    let _mouse_position @ (mouse_x, mouse_y): &(u32, u32) = &app
        .app_state
        .last_iteration
        .last_known_mouse_position
        .clone();

    let _surface_size @ PhysicalSize { width, height }: PhysicalSize<u32> = surface_size;

    let (mouse_x, mouse_y) = (*mouse_x as i32, *mouse_y as i32);
    let mouse_position: &(i32, i32) = &(mouse_x, mouse_y);
    let texture = app.systems.wgpu.device.create_texture(&TextureDescriptor {
        label: Some("overlays"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let mut found_hover = false;

    // The user interface is only displayed when the mouse is over the screen.
    if app.mirror.render.should_render_ui(&app) {
        if *&app.app_state.intricate_todo_refactor.crop_button_pressed {
            /*           let x: i32 = (width as i32 / 2)
                - (binary_images::ICON_SELECT_SCREEN_AREA.dimensions.width / 2) as i32;
            let y: i32 = (height as i32 / 2)
                - (binary_images::ICON_SELECT_SCREEN_AREA.dimensions.height / 2) as i32;

            write_image_to_texture(
                state,
                &texture,
                &binary_images::ICON_SELECT_SCREEN_AREA,
                (x, y),
            ); */
        } else if *&app.user_interaction.mouse_over_screen {
            // settings button
            {
                let img_position = (
                    0 as i32 + ((0 as i32 + 10) * 1),
                    height as i32
                        - (binary_images::ICON_PIP_NO_FILL.dimensions.height as i32 + 5 * 1),
                );

                if mouse_in_img_bounds(
                    &mouse_position,
                    &img_position,
                    &binary_images::ICON_GEAR_NO_FILL,
                    &mut found_hover,
                ) {
                    write_image_to_texture(
                        app,
                        &texture,
                        &binary_images::ICON_GEAR_FILL,
                        img_position,
                    );
                } else {
                    write_image_to_texture(
                        app,
                        &texture,
                        &binary_images::ICON_GEAR_NO_FILL,
                        img_position,
                    );
                }

                if found_remove_mouse_click(
                    &mut app.user_interaction.mouse_clicks,
                    &img_position,
                    &binary_images::ICON_GEAR_NO_FILL,
                ) {
                    let _ = app.external.settings_ui.gtk_open_signal(app);
                }
            }

            // crop button
            {
                let img_position = (
                    10 as i32
                        + ((binary_images::ICON_PIP_NO_FILL.dimensions.height as i32 + 10) * 1),
                    height as i32
                        - (binary_images::ICON_PIP_NO_FILL.dimensions.height as i32 + 5 * 1),
                );

                if mouse_in_img_bounds(
                    &mouse_position,
                    &img_position,
                    &binary_images::ICON_PIP_NO_FILL,
                    &mut found_hover,
                ) {
                    write_image_to_texture(
                        app,
                        &texture,
                        &binary_images::ICON_PIP_FILL,
                        img_position,
                    );
                } else {
                    write_image_to_texture(
                        app,
                        &texture,
                        &binary_images::ICON_PIP_NO_FILL,
                        img_position,
                    );
                }

                if found_remove_mouse_click(
                    &mut app.user_interaction.mouse_clicks,
                    &img_position,
                    &binary_images::ICON_PIP_NO_FILL,
                ) {
                    start_crop_selection(app);
                }
            }

            let temp = app.configuration.active.clone();
            let temp = temp.lossy_into_set_ui();
            let mut active_cfg: CreateUiState = temp.into();

            let idx = app.configuration.active.active_profile;

            let profile_list = app.configuration.profiles.list();

            let mut in_use_profile = profile_list
                .get(idx)
                .map(|v| v.clone())
                .unwrap_or(profile_list.last().unwrap().clone())
                .clone()
                .config;

            // These failover and shouldn't be compared to the user's preferences.
            active_cfg.present = None;
            in_use_profile.present = None;

            // trash button
            if in_use_profile != active_cfg {
                {
                    let img_position = (
                        10 as i32
                            + ((binary_images::ICON_TRASH_NO_FILL.dimensions.height as i32 + 10)
                                * 2),
                        height as i32
                            - (binary_images::ICON_TRASH_NO_FILL.dimensions.height as i32 + 5 * 1),
                    );

                    if mouse_in_img_bounds(
                        &mouse_position,
                        &img_position,
                        &binary_images::ICON_TRASH_NO_FILL,
                        &mut found_hover,
                    ) {
                        write_image_to_texture(
                            app,
                            &texture,
                            &binary_images::ICON_TRASH_FILL,
                            img_position,
                        );
                    } else {
                        write_image_to_texture(
                            app,
                            &texture,
                            &binary_images::ICON_TRASH_NO_FILL,
                            img_position,
                        );
                    }

                    if found_remove_mouse_click(
                        &mut app.user_interaction.mouse_clicks,
                        &img_position,
                        &binary_images::ICON_TRASH_NO_FILL,
                    ) {
                        let profiles = load_profiles().unwrap_or(Default::default());
                        let using_profile =
                            profiles.get(app.configuration.active.active_profile as usize);

                        let idx = app.configuration.active.active_profile;

                        let temp = &mut app.configuration.active;
                        let temp = temp.update();

                        *temp = using_profile.config.clone().into();

                        temp.reload_profiles = true;
                        temp.active_profile = idx as usize;

                        app.app_state.intricate_todo_refactor.new_settings = true;

                        app.external
                            .channels
                            .gpu_sender_request
                            .send(app.configuration.active.clone())
                            .unwrap();
                    }
                }
            }

            // profile button
            if profile_list.len() > 1 {
                {
                    let img_position = (
                        width as i32
                            - ((binary_images::ICON_DIAMOND_PROFILE_NO_FILL
                                .dimensions
                                .height as i32
                                + 10)
                                * 1),
                        height as i32
                            - (binary_images::ICON_DIAMOND_PROFILE_NO_FILL
                                .dimensions
                                .height as i32
                                + 7 * 1),
                    );

                    if mouse_in_img_bounds(
                        &mouse_position,
                        &img_position,
                        &binary_images::ICON_DIAMOND_PROFILE_NO_FILL,
                        &mut found_hover,
                    ) {
                        write_image_to_texture(
                            app,
                            &texture,
                            &binary_images::ICON_DIAMOND_PROFILE_FILL,
                            img_position,
                        );
                    } else {
                        write_image_to_texture(
                            app,
                            &texture,
                            &binary_images::ICON_DIAMOND_PROFILE_NO_FILL,
                            img_position,
                        );
                    }

                    if found_remove_mouse_click(
                        &mut app.user_interaction.mouse_clicks,
                        &img_position,
                        &binary_images::ICON_DIAMOND_PROFILE_NO_FILL,
                    ) {
                        app.configuration.profiles = load_profiles().unwrap_or(Default::default());
                        let profile_list = app.configuration.profiles.list();

                        let next = (idx + 1) % profile_list.len();

                        let next_profile = profile_list
                            .get(next)
                            .map(|v| v.clone())
                            .unwrap_or(profile_list.last().unwrap().clone())
                            .clone()
                            .config;

                        let mut as_state: UiState = next_profile.into();
                        as_state.active_profile = next;

                        app.configuration.active = as_state;
                        app.app_state.intricate_todo_refactor.new_settings = true;

                        app.external
                            .channels
                            .gpu_sender_request
                            .send(app.configuration.active.clone())
                            .unwrap();
                    }
                }
            }
        }

        if *&app.user_interaction.mouse_over_screen {
            if let TitleBarDisplay::HiddenTitleBar = ui_settings.display_title {
                // exit button
                {
                    let img_position = (width as i32 - (35 * 1), 10);

                    if mouse_in_img_bounds(
                        &mouse_position,
                        &img_position,
                        &binary_images::ICON_EXIT_NO_FILL,
                        &mut found_hover,
                    ) {
                        write_image_to_texture(
                            app,
                            &texture,
                            &binary_images::ICON_EXIT_FILL,
                            img_position,
                        );
                    } else {
                        write_image_to_texture(
                            app,
                            &texture,
                            &binary_images::ICON_EXIT_NO_FILL,
                            img_position,
                        );
                    }

                    if found_remove_mouse_click(
                        &mut app.user_interaction.mouse_clicks,
                        &img_position,
                        &binary_images::ICON_EXIT_NO_FILL,
                    ) {
                        shutdown::start_shutdown(app);
                    }
                }

                // maximize button
                {
                    let img_position = (width as i32 - (35 * 2), 10);

                    if mouse_in_img_bounds(
                        &mouse_position,
                        &img_position,
                        &binary_images::ICON_SQUARE_NO_FILL,
                        &mut found_hover,
                    ) {
                        write_image_to_texture(
                            app,
                            &texture,
                            &binary_images::ICON_SQUARE_FILL,
                            img_position,
                        );
                    } else {
                        write_image_to_texture(
                            app,
                            &texture,
                            &binary_images::ICON_SQUARE_NO_FILL,
                            img_position,
                        );
                    }

                    if found_remove_mouse_click(
                        &mut app.user_interaction.mouse_clicks,
                        &img_position,
                        &binary_images::ICON_SQUARE_NO_FILL,
                    ) {
                        if !app.systems.window.is_maximized() {
                            app.systems.window.set_maximized(true);
                        } else {
                            app.systems.window.set_maximized(false);
                        }
                    }
                }

                // minimize button
                {
                    let img_position = (width as i32 - (35 * 3), 10);

                    if mouse_in_img_bounds(
                        &mouse_position,
                        &img_position,
                        &binary_images::ICON_MINIMIZE_NO_FILL,
                        &mut found_hover,
                    ) {
                        write_image_to_texture(
                            app,
                            &texture,
                            &binary_images::ICON_MINIMIZE_FILL,
                            img_position,
                        );
                    } else {
                        write_image_to_texture(
                            app,
                            &texture,
                            &binary_images::ICON_MINIMIZE_NO_FILL,
                            img_position,
                        );
                    }

                    if found_remove_mouse_click(
                        &mut app.user_interaction.mouse_clicks,
                        &img_position,
                        &binary_images::ICON_MINIMIZE_NO_FILL,
                    ) {
                        app.systems.window.set_minimized(true);
                    }
                }
            }

            if !*&app.app_state.intricate_todo_refactor.crop_button_pressed {
                if !found_hover {
                    if let Some(idx) = first_in_range(
                        &mut app.user_interaction.mouse_downs,
                        &((0, 0), ((width as i32), (height as i32))),
                    ) {
                        let _ = app.systems.window.drag_window();
                        app.user_interaction.mouse_downs.remove(idx);
                    }
                }
            }
        }
    }

    texture.create_view(&TextureViewDescriptor::default())
}

#[derive(Clone, Copy, Debug)]
pub enum UiFlag {
    #[allow(unused)]
    NoFlag = 0,
    DisplayOverlays = 1 << 0,
    MouseOverWindow = 1 << 1,
    MouseDown = 1 << 2,
    WaitingForCrop = 1 << 3,
    OnlyAngles = 1 << 4,
    KeepBorders = 1 << 5,
    UseGreenScreen = 1 << 6,
}

#[derive(Clone, Debug)]

pub struct UiRenderData {
    flagged: Vec<UiFlag>,
    transparency: f32,
    mouse_position: (u32, u32),
    surface_dimensions: (u32, u32),
    mouse_select_start: (u32, u32),
    mirror_out_start: (u32, u32),
    mirror_out_end: (u32, u32),
    greenscreen: Option<RemoveColors>,
}

impl Into<u32> for UiFlag {
    fn into(self) -> u32 {
        self as u32
    }
}

impl Into<Vec<u8>> for UiRenderData {
    fn into(self) -> Vec<u8> {
        let data: u32 = self
            .flagged
            .iter()
            .map(|v| (*v).into())
            .fold(0, |a, i: u32| a | i);

        let RemoveColors {
            base_color: (gs_r, gs_g, gs_b),
            sensitivity: gs_sensitivity,
        } = if let Some(v) = self.greenscreen {
            v
        } else {
            RemoveColors {
                base_color: (0.0, 0.0, 0.0),
                sensitivity: (0.0),
            }
        };

        let time = START_TIME.elapsed().as_secs_f32();

        let mut time = time.to_le_bytes().to_vec();
        let mut gs_r = gs_r.to_le_bytes().to_vec();
        let mut gs_g = gs_g.to_le_bytes().to_vec();
        let mut gs_b = gs_b.to_le_bytes().to_vec();
        let mut gs_sensitivity = gs_sensitivity.to_le_bytes().to_vec();

        let mut surface_end_x: Vec<u8> = self.mirror_out_end.0.to_le_bytes().to_vec();
        let mut surface_end_y = self.mirror_out_end.1.to_le_bytes().to_vec();
        let mut surface_start_x = self.mirror_out_start.0.to_le_bytes().to_vec();
        let mut surface_start_y = self.mirror_out_start.1.to_le_bytes().to_vec();
        let mut select_x = self.mouse_select_start.0.to_le_bytes().to_vec();
        let mut select_y = self.mouse_select_start.1.to_le_bytes().to_vec();
        let mut surface_w = self.surface_dimensions.0.to_le_bytes().to_vec();
        let mut surface_h = self.surface_dimensions.1.to_le_bytes().to_vec();
        let mut mouse_x = self.mouse_position.0.to_le_bytes().to_vec();
        let mut mouse_y = self.mouse_position.1.to_le_bytes().to_vec();
        let mut transparency = self.transparency.to_le_bytes().to_vec();
        let mut flags = data.to_le_bytes().to_vec();

        flags.append(&mut transparency);
        flags.append(&mut mouse_y);
        flags.append(&mut mouse_x);
        flags.append(&mut surface_h);
        flags.append(&mut surface_w);
        flags.append(&mut select_y);
        flags.append(&mut select_x);
        flags.append(&mut surface_start_x);
        flags.append(&mut surface_start_y);
        flags.append(&mut surface_end_x);
        flags.append(&mut surface_end_y);
        flags.append(&mut gs_r);
        flags.append(&mut gs_g);
        flags.append(&mut gs_b);
        flags.append(&mut gs_sensitivity);
        flags.append(&mut time);

        flags
    }
}

pub fn write_ui_data_to_buffer(
    queue: &Queue,
    _surface_size @ PhysicalSize { width, height }: PhysicalSize<u32>,
    mouse: (u32, u32),
    mouse_select_start: (u32, u32),
    buffer: &wgpu::Buffer,
    transparency: f32,
    flags: &Vec<UiFlag>,
    (surface_start_x, surface_start_y): (u32, u32),
    defined_ends: Option<(u32, u32)>,
    settings: Option<RemoveColors>,
) {
    let (s_end_x, s_end_y) = if let Some(val) = defined_ends {
        val
    } else {
        (width, height)
    };

    let temp = UiRenderData {
        flagged: flags.clone(),
        transparency: transparency / 100.0,
        mouse_position: mouse,
        surface_dimensions: (width, height),
        mouse_select_start: mouse_select_start,
        mirror_out_start: (surface_start_x, surface_start_y),
        mirror_out_end: (s_end_x, s_end_y),
        greenscreen: settings,
    };

    let temp: Vec<u8> = temp.into();

    queue.write_buffer(&buffer, 0, &temp);
}
