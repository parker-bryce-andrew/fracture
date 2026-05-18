use super::defaults::{CROP_COLOR, SELECTION_WINDOW_OFFSETS};
use crate::{
    global_application_state::LastReported,
    gpu_mirror_display::state::Application,
    ui_state::{
        ScaleDecision, TitleBarDisplay, UiState, VideoAspect, VideoLocation, WindowBackground,
        WindowBehaviour,
    },
};
use std::sync::Arc;
use winit::dpi::PhysicalSize;

pub fn start_crop_selection(app: &mut Application) {
    app.systems.window.set_maximized(false);
    app.systems.window.set_minimized(false);

    app.app_state.intricate_todo_refactor.crop_button_pressed = true;
    app.app_state.intricate_todo_refactor.new_settings = true;

    app.configuration = UiState {
        display_title: TitleBarDisplay::TitleBarVisible,
        aspect_ratio: VideoAspect::MaintainAspectRatio(
            ScaleDecision::DontScale,
            WindowBehaviour::SizeSetByUser(VideoLocation::Center),
        ),
        frame_transparency: 100.0,
        need_rebuild: true,
        updated: true,

        green_screen: crate::ui_state::GreenScreen::None,
        postprocessor: Default::default(),
        background: WindowBackground::Color(CROP_COLOR.0, CROP_COLOR.1, CROP_COLOR.2, CROP_COLOR.3),
        ..Default::default()
    };

    app.external
        .channels
        .gpu_sender_request
        .send(app.configuration.clone())
        .unwrap();

    let _ = app.external.settings_ui.gtk_shutdown_signal_checked(&app);

    app.app_state.cropped = Some(CroppedArea {
        relative_to_window_position: InitialAbsoluteWindowPosition { x: 0, y: 0 },
        size: Size {
            width: app.app_state.last_iteration.last_frame_size.0,
            height: app.app_state.last_iteration.last_frame_size.1,
        },
        relative_to_frame_position: InitialAbsoluteFramePosition {
            x: app.app_state.last_iteration.last_reported_offsets.0,
            y: app.app_state.last_iteration.last_reported_offsets.1,
        },
    });

    app.systems
        .window
        .request_inner_size(PhysicalSize {
            width: app.app_state.cropped.as_ref().unwrap().size.width + SELECTION_WINDOW_OFFSETS.0,
            height: app.app_state.cropped.as_ref().unwrap().size.height
                + SELECTION_WINDOW_OFFSETS.1,
        })
        .unwrap();
}

pub fn if_crop_button_is_active(
    app: &mut Application,
    crop_button_press: &bool,
    frame: &Arc<LastReported>,
) {
    let cropped_button_press: bool = *crop_button_press;

    if cropped_button_press {
        if app.systems.window.is_maximized() {
            app.systems.window.set_maximized(false);
        }

        let PhysicalSize { width, height } = app.systems.window.inner_size();
        let (f_w, f_h) = frame.window_dimensions;
        let (off_w, off_h) = SELECTION_WINDOW_OFFSETS;

        if f_w + off_w != width || f_h + off_h != height {
            app.app_state.cropped = Some(CroppedArea {
                relative_to_window_position: InitialAbsoluteWindowPosition { x: 0, y: 0 },
                size: Size {
                    width: frame.window_dimensions.0,
                    height: frame.window_dimensions.1,
                },
                relative_to_frame_position: InitialAbsoluteFramePosition {
                    x: app.app_state.last_iteration.last_reported_offsets.0,
                    y: app.app_state.last_iteration.last_reported_offsets.1,
                },
            });

            app.systems
                .window
                .request_inner_size(PhysicalSize {
                    width: f_w + off_w,
                    height: f_h + off_h,
                })
                .unwrap();
        }
    }
}
#[derive(Debug, PartialEq)]
pub enum CropEndTriggeredFrom {
    MouseUp,
    EnterPress,
}

#[derive(Clone, Debug)]
pub struct InitialAbsoluteWindowPosition {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Debug)]
pub struct InitialAbsoluteFramePosition {
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Debug)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct CroppedArea {
    /// The x and y values are from mouse clicks on the window that is created
    /// by the application. This window size changes over time.
    pub relative_to_window_position: InitialAbsoluteWindowPosition,
    pub relative_to_frame_position: InitialAbsoluteFramePosition,
    /// A cro
    pub size: Size,
}

pub fn if_in_crop_complete_crop(app: &mut Application, from: CropEndTriggeredFrom) {
    let PhysicalSize { width, height } = app.systems.window.inner_size();

    if app.app_state.intricate_todo_refactor.in_crop_selection
        || app.app_state.intricate_todo_refactor.crop_button_pressed
            && (from == CropEndTriggeredFrom::EnterPress)
    {
        let (min_x, max_x) = {
            match from {
                CropEndTriggeredFrom::MouseUp => (
                    app.app_state
                        .last_iteration
                        .last_known_mouse_position
                        .0
                        .min(app.user_interaction.mouse_select_start.0),
                    app.app_state
                        .last_iteration
                        .last_known_mouse_position
                        .0
                        .max(app.user_interaction.mouse_select_start.0),
                ),
                CropEndTriggeredFrom::EnterPress => (0, width),
            }
        };

        let (min_y, max_y) = {
            match from {
                CropEndTriggeredFrom::MouseUp => (
                    app.app_state
                        .last_iteration
                        .last_known_mouse_position
                        .1
                        .min(app.user_interaction.mouse_select_start.1),
                    app.app_state
                        .last_iteration
                        .last_known_mouse_position
                        .1
                        .max(app.user_interaction.mouse_select_start.1),
                ),
                CropEndTriggeredFrom::EnterPress => (0, height),
            }
        };

        let min_x: i32 = (min_x as i32 - ((SELECTION_WINDOW_OFFSETS.0 / 2) as i32)).max(0);
        let min_y: i32 = (min_y as i32 - ((SELECTION_WINDOW_OFFSETS.1 / 2) as i32)).max(0);

        let max_x: i32 = (max_x as i32 - ((SELECTION_WINDOW_OFFSETS.0 / 2) as i32)).max(0);
        let max_y: i32 = (max_y as i32 - ((SELECTION_WINDOW_OFFSETS.1 / 2) as i32)).max(0);

        let pos = (min_x as u32, min_y as u32);

        let window_position = InitialAbsoluteWindowPosition { x: pos.0, y: pos.1 };

        app.app_state.cropped = Some(CroppedArea {
            relative_to_frame_position: InitialAbsoluteFramePosition {
                x: window_position.x + app.app_state.last_iteration.last_reported_offsets.0,
                y: window_position.y + app.app_state.last_iteration.last_reported_offsets.1,
            },
            relative_to_window_position: window_position,
            size: Size {
                width: ((max_x as i32 - min_x as i32) + 1 as i32)
                    .min(app.app_state.last_iteration.last_frame_size.0 as i32 - pos.0 as i32)
                    .max(0) as u32,
                height: ((max_y as i32 - min_y as i32) + 1 as i32)
                    .min(app.app_state.last_iteration.last_frame_size.1 as i32 - pos.1 as i32)
                    .max(0) as u32,
            },
        });

        if app.app_state.cropped.as_ref().unwrap().size.width <= 5
            || app.app_state.cropped.as_ref().unwrap().size.height <= 5
        {
            // This is kinda a hack to force small selections and entirely offscreen selections to fullscreen. I'm doubtful it will always work,
            // and it seems very likely to crash. I think the section of code that crops and repositions the frame
            // has an off by 1 error that will core dump on very small selections (like 1-5 pixels). There's likely still
            // other unfound crashes with resizing because of it, but the desired behaviour is for very small
            // crops (and entirely offscreen crops) to snap to the entire window size.
            //
            // I'm not fixing it at this time because I want to focus on more important issues before spending time trying to determine
            // what is wrong with it. If this is commented out, (The if statement is removed, and the else is left to be called always)
            // then a 1 pixel selection is made, the crash will happen.
            if_in_crop_complete_crop(app, CropEndTriggeredFrom::EnterPress);
        } else {
            app.configuration = UiState {
                display_title: TitleBarDisplay::HiddenTitleBar,
                aspect_ratio: VideoAspect::MaintainAspectRatio(
                    ScaleDecision::Scale,
                    WindowBehaviour::SizeMatchesMirrorAspect,
                ),
                frame_transparency: 100.0,
                need_rebuild: true,
                updated: true,
                green_screen: crate::ui_state::GreenScreen::None,
                postprocessor: Default::default(),
                ..Default::default()
            };

            app.app_state.intricate_todo_refactor.new_settings = true;

            app.systems
                .window
                .request_inner_size(PhysicalSize {
                    width: app.app_state.cropped.as_ref().unwrap().size.width,
                    height: app.app_state.cropped.as_ref().unwrap().size.height,
                })
                .unwrap();

            app.external
                .channels
                .gpu_sender_request
                .send(app.configuration.clone())
                .unwrap();
        }

        app.app_state.intricate_todo_refactor.in_crop_selection = false;
        app.app_state.intricate_todo_refactor.crop_button_pressed = false;
    }
}
