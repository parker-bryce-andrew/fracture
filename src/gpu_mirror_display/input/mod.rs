use crate::gpu_mirror_display::state::Application;
use events_keyboard::on_keyboard_events;
use events_mouse::on_mouse_events;
use winit::event::WindowEvent;

pub(crate) mod events_keyboard;
pub(crate) mod events_mouse;
pub(crate) mod utility_mouse;

pub(crate) fn on_input_events(app: &mut Application, event: &WindowEvent) {
    on_mouse_events(app, event);
    on_keyboard_events(app, event);
}
