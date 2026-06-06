use crate::gpu_mirror_display::{
    state::Application,
    window_cropping::{CropEndTriggeredFrom, if_in_crop_complete_crop},
};
use winit::{
    event::{KeyEvent, WindowEvent},
    keyboard::KeyCode,
};

fn if_user_defined_shortcut(app: &mut Application, ev: &KeyEvent) -> bool {
    match &ev.text {
        Some(key_released) => match ev.state {
            winit::event::ElementState::Pressed => return false,
            winit::event::ElementState::Released => {
                let profile =
                    app.configuration
                        .profiles
                        .list()
                        .iter()
                        .enumerate()
                        .find(|(_, v)| {
                            if let Some(c) = v.shortcut {
                                key_released
                                    .eq(&winit::keyboard::SmolStr::new_inline(&c.to_string()))
                            } else {
                                false
                            }
                        });

                if let Some((idx, _)) = profile {
                    app.configuration
                        .load_profile(idx, &mut app.app_state, &app.external);

                    return true;
                } else {
                    return false;
                }
            }
        },
        None => return false,
    }
}

pub(crate) fn on_keyboard_events(app: &mut Application, event: &WindowEvent) {
    match event {
        WindowEvent::KeyboardInput {
            device_id: _,
            event,
            is_synthetic: _,
        } => {
            match event.state {
                winit::event::ElementState::Pressed => match event.physical_key {
                    winit::keyboard::PhysicalKey::Code(key_code) => match key_code {
                        KeyCode::ShiftLeft => {
                            app.user_interaction.shift_left_is_down = true;
                        }
                        KeyCode::ShiftRight => {
                            app.user_interaction.shift_right_is_down = true;
                        }
                        _ => {}
                    },
                    winit::keyboard::PhysicalKey::Unidentified(_) => {}
                },
                winit::event::ElementState::Released => match event.physical_key {
                    winit::keyboard::PhysicalKey::Code(key_code) => match key_code {
                        KeyCode::NumpadEnter | KeyCode::Enter => {
                            if_in_crop_complete_crop(app, CropEndTriggeredFrom::EnterPress);
                        }
                        KeyCode::Escape => {
                            // todo: Should something happen on escape?
                        }
                        KeyCode::KeyF => {
                            if app.app_state.initialization_checks.track_fps {
                                println!(
                                    "------------------------------------------------------------------"
                                );
                                println!("{:?}", app.metrics.fps_tracking.report());
                                println!("{:?}", app.systems.wgpu.surface.get_configuration());
                            }
                        }

                        KeyCode::KeyP => {
                            if app.app_state.initialization_checks.track_fps {
                                let _ = app.external.channels.request_pipewire_fps.send(());
                            }
                        }
                        KeyCode::ShiftLeft => {
                            app.user_interaction.shift_left_is_down = false;
                        }
                        KeyCode::ShiftRight => {
                            app.user_interaction.shift_right_is_down = false;
                        }
                        _ => {}
                    },
                    winit::keyboard::PhysicalKey::Unidentified(_) => {}
                },
            }

            match { *&app.app_state.intricate_todo_refactor.crop_button_pressed } {
                false => match event.state {
                    winit::event::ElementState::Pressed => {}
                    winit::event::ElementState::Released => {
                        if if_user_defined_shortcut(app, &event) {
                            return;
                        }

                        if let Some(text) = &event.text {
                            if let Some(c) = text.as_str().chars().next() {
                                match c {
                                    '<' => {
                                        app.configuration.load_previous_rotation(
                                            &mut app.app_state,
                                            &app.external,
                                        );
                                    }
                                    '>' => {
                                        app.configuration
                                            .load_next_rotation(&mut app.app_state, &app.external);
                                    }
                                    _ => {}
                                }
                            }
                        }

                        match event.physical_key {
                            winit::keyboard::PhysicalKey::Code(key_code) => match key_code {
                                KeyCode::Digit0 => {
                                    app.configuration.load_rotation_profile(
                                        10 - 1,
                                        &mut app.app_state,
                                        &app.external,
                                    );
                                }
                                KeyCode::Digit1 => {
                                    app.configuration.load_rotation_profile(
                                        1 - 1,
                                        &mut app.app_state,
                                        &app.external,
                                    );
                                }
                                KeyCode::Digit2 => {
                                    app.configuration.load_rotation_profile(
                                        2 - 1,
                                        &mut app.app_state,
                                        &app.external,
                                    );
                                }
                                KeyCode::Digit3 => {
                                    app.configuration.load_rotation_profile(
                                        3 - 1,
                                        &mut app.app_state,
                                        &app.external,
                                    );
                                }
                                KeyCode::Digit4 => {
                                    app.configuration.load_rotation_profile(
                                        4 - 1,
                                        &mut app.app_state,
                                        &app.external,
                                    );
                                }
                                KeyCode::Digit5 => {
                                    app.configuration.load_rotation_profile(
                                        5 - 1,
                                        &mut app.app_state,
                                        &app.external,
                                    );
                                }
                                KeyCode::Digit6 => {
                                    app.configuration.load_rotation_profile(
                                        6 - 1,
                                        &mut app.app_state,
                                        &app.external,
                                    );
                                }
                                KeyCode::Digit7 => {
                                    app.configuration.load_rotation_profile(
                                        7 - 1,
                                        &mut app.app_state,
                                        &app.external,
                                    );
                                }
                                KeyCode::Digit8 => {
                                    app.configuration.load_rotation_profile(
                                        8 - 1,
                                        &mut app.app_state,
                                        &app.external,
                                    );
                                }
                                KeyCode::Digit9 => {
                                    app.configuration.load_rotation_profile(
                                        9 - 1,
                                        &mut app.app_state,
                                        &app.external,
                                    );
                                }

                                _ => {}
                            },
                            winit::keyboard::PhysicalKey::Unidentified(_) => {}
                        }
                    }
                },
                true => {}
            }
        }

        _ => {}
    }
}
