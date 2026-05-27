use crate::gpu_mirror_display::{
    state::Application,
    window_cropping::{CropEndTriggeredFrom, if_in_crop_complete_crop},
};
use winit::{event::WindowEvent, keyboard::KeyCode};

pub(crate) fn on_keyboard_events(app: &mut Application, event: &WindowEvent) {
    match event {
        WindowEvent::KeyboardInput {
            device_id: _,
            event,
            is_synthetic: _,
        } => match event.physical_key {
            winit::keyboard::PhysicalKey::Code(key_code) => match key_code {
                KeyCode::NumpadEnter | KeyCode::Enter => {
                    if_in_crop_complete_crop(app, CropEndTriggeredFrom::EnterPress);
                }
                KeyCode::Escape => {
                    // todo: Should something happen on escape?
                }
                KeyCode::KeyF => match event.state {
                    winit::event::ElementState::Released => {
                        if app.app_state.initialization_checks.track_fps {
                            println!(
                                "------------------------------------------------------------------"
                            );
                            println!("{:?}", app.metrics.fps_tracking.report());
                            println!("{:?}", app.systems.wgpu.surface.get_configuration());
                        }
                    }
                    _ => {}
                },
                KeyCode::KeyP => match event.state {
                    winit::event::ElementState::Released => {
                        if app.app_state.initialization_checks.track_fps {
                            let _ = app.external.channels.request_pipewire_fps.send(());
                        }
                    }
                    _ => {}
                },
                _ => {}
            },
            _ => {}
        },

        _ => {}
    }
}
