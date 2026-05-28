use crate::{
    gpu_mirror_display::{
        state::Application,
        window_cropping::{CropEndTriggeredFrom, if_in_crop_complete_crop},
    },
    ui_state::{TitleBarDisplay, VideoAspect, WindowBehaviour},
};
use std::time::SystemTime;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::WindowEvent,
};

#[derive(PartialEq, Eq)]
pub enum ResizeInteractionsState {
    None,
    NwResize,
    SwResize,
    NeResize,
    SeResize,
    North,
    South,
    East,
    West,
}

pub(crate) fn on_mouse_events(app: &mut Application, event: &WindowEvent) {
    match event {
        WindowEvent::MouseInput {
            device_id: _,
            state: state2,
            button: btn,
        } => match state2 {
            winit::event::ElementState::Pressed => match btn {
                winit::event::MouseButton::Left => {
                    app.app_state.intricate_todo_refactor.keep_borders = true;

                    match app.user_interaction.mouse_resize_state {
                        ResizeInteractionsState::None => {
                            app.app_state.intricate_todo_refactor.keep_borders = false;
                            app.user_interaction.mouse_downs.push((
                                (app.app_state.last_iteration.last_known_mouse_position),
                                SystemTime::now(),
                            ));

                            if app.app_state.intricate_todo_refactor.crop_button_pressed {
                                app.app_state.intricate_todo_refactor.in_crop_selection = true;
                            }

                            app.user_interaction.mouse_is_down = true;
                            app.user_interaction.mouse_select_start =
                                app.app_state.last_iteration.last_known_mouse_position;
                        }
                        ResizeInteractionsState::NwResize => {
                            let _ = app
                                .systems
                                .window
                                .drag_resize_window(winit::window::ResizeDirection::NorthWest);
                        }
                        ResizeInteractionsState::SwResize => {
                            let _ = app
                                .systems
                                .window
                                .drag_resize_window(winit::window::ResizeDirection::SouthWest);
                        }
                        ResizeInteractionsState::NeResize => {
                            let _ = app
                                .systems
                                .window
                                .drag_resize_window(winit::window::ResizeDirection::NorthEast);
                        }
                        ResizeInteractionsState::SeResize => {
                            let _ = app
                                .systems
                                .window
                                .drag_resize_window(winit::window::ResizeDirection::SouthEast);
                        }
                        ResizeInteractionsState::North => {
                            let _ = app
                                .systems
                                .window
                                .drag_resize_window(winit::window::ResizeDirection::North);
                        }
                        ResizeInteractionsState::South => {
                            let _ = app
                                .systems
                                .window
                                .drag_resize_window(winit::window::ResizeDirection::South);
                        }
                        ResizeInteractionsState::East => {
                            let _ = app
                                .systems
                                .window
                                .drag_resize_window(winit::window::ResizeDirection::East);
                        }
                        ResizeInteractionsState::West => {
                            let _ = app
                                .systems
                                .window
                                .drag_resize_window(winit::window::ResizeDirection::West);
                        }
                    }
                }
                winit::event::MouseButton::Right => {
                    app.systems.window.show_window_menu(PhysicalPosition {
                        x: app.app_state.last_iteration.last_known_mouse_position.0,
                        y: app.app_state.last_iteration.last_known_mouse_position.1,
                    });
                }

                _ => {}
            },
            winit::event::ElementState::Released => match btn {
                winit::event::MouseButton::Left => {
                    app.user_interaction.mouse_clicks.push((
                        (app.app_state.last_iteration.last_known_mouse_position),
                        SystemTime::now(),
                    ));

                    app.user_interaction.mouse_is_down = false;

                    if_in_crop_complete_crop(app, CropEndTriggeredFrom::MouseUp);
                }
                _ => {}
            },
        },

        WindowEvent::CursorMoved {
            device_id: _,
            position,
        } => {
            on_cursor_movements(app, position);
        }

        WindowEvent::CursorEntered { device_id: _ } => {
            app.user_interaction.mouse_over_screen = true;
        }
        WindowEvent::Focused(value) => match value {
            true => {}
            false => {
                app.user_interaction.mouse_is_down = false;
                app.app_state.intricate_todo_refactor.in_crop_selection = false;
            }
        },
        WindowEvent::CursorLeft { device_id: _ } => {
            app.user_interaction.mouse_over_screen = false;
            {
                // if there's a crop happening then keep rendering the selection
                // until focus from the mouse is lost.
                if !((app.app_state.intricate_todo_refactor.in_crop_selection
                    || app.app_state.intricate_todo_refactor.crop_button_pressed)
                    && app.systems.window.has_focus())
                {
                    app.user_interaction.mouse_is_down = false;
                }
            }
        }
        _ => {}
    }
}

fn on_cursor_movements(app: &mut Application, position: &PhysicalPosition<f64>) {
    app.app_state.intricate_todo_refactor.keep_borders = false;

    app.app_state.last_iteration.last_known_mouse_position =
        (position.x.round() as u32, position.y.round() as u32);

    let PhysicalSize { width, height } = app.systems.window.inner_size();

    let (x, y) = *&app.app_state.last_iteration.last_known_mouse_position;
    let (x, y) = (x as i32, y as i32);
    let (width, height) = (width as i32, height as i32);

    let resize = 10;

    if app.configuration.active.display_title == TitleBarDisplay::HiddenTitleBar {
        if x < resize && y < resize {
            app.systems
                .window
                .set_cursor(winit::window::CursorIcon::NwResize);
            app.user_interaction.mouse_resize_state = ResizeInteractionsState::NwResize;
        } else if x < resize && y > height - resize {
            app.systems
                .window
                .set_cursor(winit::window::CursorIcon::SwResize);
            app.user_interaction.mouse_resize_state = ResizeInteractionsState::SwResize;
        } else if y < resize && x > width - resize {
            app.systems
                .window
                .set_cursor(winit::window::CursorIcon::NeResize);
            app.user_interaction.mouse_resize_state = ResizeInteractionsState::NeResize;
        } else if y > height - resize && x > width - resize {
            app.systems
                .window
                .set_cursor(winit::window::CursorIcon::SeResize);
            app.user_interaction.mouse_resize_state = ResizeInteractionsState::SeResize;
        } else {
            if let VideoAspect::MaintainAspectRatio(_, WindowBehaviour::SizeMatchesMirrorAspect) =
                app.configuration.active.aspect_ratio
            {
                app.systems
                    .window
                    .set_cursor(winit::window::CursorIcon::Default);
                app.user_interaction.mouse_resize_state = ResizeInteractionsState::None;
            } else {
                if x < resize || x > width - resize || y < resize || y > height - resize {
                    if x < resize || x > width - resize {
                        app.systems
                            .window
                            .set_cursor(winit::window::CursorIcon::ColResize);

                        if x < resize {
                            app.user_interaction.mouse_resize_state = ResizeInteractionsState::West;
                        } else {
                            app.user_interaction.mouse_resize_state = ResizeInteractionsState::East;
                        }
                    } else {
                        app.systems
                            .window
                            .set_cursor(winit::window::CursorIcon::RowResize);

                        if y < resize {
                            app.user_interaction.mouse_resize_state =
                                ResizeInteractionsState::North;
                        } else {
                            app.user_interaction.mouse_resize_state =
                                ResizeInteractionsState::South;
                        }
                    }
                } else {
                    app.systems
                        .window
                        .set_cursor(winit::window::CursorIcon::Default);
                    app.user_interaction.mouse_resize_state = ResizeInteractionsState::None;
                }
            }
        }
    } else {
        app.systems
            .window
            .set_cursor(winit::window::CursorIcon::Default);
        app.user_interaction.mouse_resize_state = ResizeInteractionsState::None;
    }
}
