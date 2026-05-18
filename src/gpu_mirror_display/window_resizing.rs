use super::window_cropping::CroppedArea;
use crate::{
    gpu_mirror_display::{
        state::Application,
        utility_vertex::{VERTICES, center_verticies, position_centered_verts},
        window_cropping::Size,
    },
    ui_state::{ScaleDecision, VideoAspect, VideoLocation, WindowBehaviour},
};
use winit::dpi::PhysicalSize;

fn if_needed_resize_window_to_scaled_frame_size(
    app: &Application,
    (frame_width, frame_height): &(u32, u32),
    (width, height): (u32, u32),
) {
    /*
        I'm just going to state that the bounding rectangle that is the window is the desired size. From
        there, I determine the largest scaled window that can fit within the bounding box, and then resize the window accordingly.

        This is different funcitonality from Firefox PIP, but I like it better because it functions predictably to the user.
    */

    if (*frame_width as u32, *frame_height as u32) != (width, height) {
        let &[_top_left, _top_right, _bottom_left, bottom_right] = &position_centered_verts(
            &center_verticies(
                VERTICES.to_vec(),
                (*frame_width as u32, *frame_height as u32),
                (width as u32, height as u32),
            ),
            &VideoLocation::NorthWest,
        )[..] else {
            panic!("Impossible");
        };

        // This is a redefinintion to make it explicit, but it is equal to the returned value.
        let top_left = [-1.0, 1.0, 0.0];
        assert_eq!(top_left, _top_left.position);

        let dist_x = bottom_right.position[0] - top_left[0];
        let dist_y = top_left[1] - bottom_right.position[1];

        let off_x = 2.0 - dist_x;
        let off_y = 2.0 - dist_y;

        let as_percent_x = 1.0 - (off_x / 2.0);
        let as_percent_y = 1.0 - (off_y / 2.0);

        let new_width = (width as f32 * as_percent_x).round() as u32;
        let new_height = (height as f32 * as_percent_y).round() as u32;

        if (new_width, new_height) != (width, height) {
            let _ = app.systems.window.request_inner_size(PhysicalSize::new(
                (new_width as i32).max(1),
                (new_height as i32).max(1),
            ));
        }
    }
}

pub fn if_needed_resize_window_to_frame_size(
    app: &Application,
    (frame_width, frame_height): &(u32, u32),
    (surface_width, surface_height): (u32, u32),
) {
    if (*frame_width as u32, *frame_height as u32) != (surface_width, surface_height) {
        let _ = app
            .systems
            .window
            .request_inner_size(PhysicalSize::new(*frame_width as i32, *frame_height as i32));
    }
}

pub fn if_surface_size_changed(app: &mut Application) {
    let last_surface_size = &mut app.app_state.last_iteration.last_surface_size;

    if *last_surface_size != app.systems.window.inner_size() {
        let (max_width, max_height) = (
            app.systems.wgpu.device.limits().max_texture_dimension_2d,
            app.systems.wgpu.device.limits().max_texture_dimension_2d,
        );

        let PhysicalSize { width, height } = app.systems.window.inner_size();

        if width > max_width {
            let _ = app
                .systems
                .window
                .request_inner_size(PhysicalSize::new(max_width as i32, height as i32));
        }

        let PhysicalSize { width, height } = app.systems.window.inner_size();

        if height > max_height {
            let _ = app
                .systems
                .window
                .request_inner_size(PhysicalSize::new(width as i32, max_height as i32));
        }

        *last_surface_size = app.systems.window.inner_size();
        app.mirror
            .render
            .resize(&mut app.systems.wgpu, app.systems.window.inner_size());
    }
}

/// if VideoAspect::MaintainAspectRatio(_, WindowBehaviour::SizeMatchesMirrorAspect)
pub fn if_settings_maintain_aspect_ratio(app: &Application, cropped: &CroppedArea) {
    if let VideoAspect::MaintainAspectRatio(
        scale_decision,
        WindowBehaviour::SizeMatchesMirrorAspect,
    ) = &app.configuration.aspect_ratio
    {
        let PhysicalSize { width, height } = app.systems.window.inner_size();
        let Size {
            width: frame_width,
            height: frame_height,
        } = cropped.size;

        match scale_decision {
            // The window (surface) size is supposed to be the exact same size as recording of the other window.
            ScaleDecision::DontScale => {
                if_needed_resize_window_to_frame_size(
                    &app,
                    &(frame_width, frame_height),
                    (width, height),
                );
            }

            // The window (surface) size changes sizes, scaling as it changes. The window automatically changes to match
            // the size of the inner scaled recorded parts. This is how Firefox pip functions
            ScaleDecision::Scale => {
                if_needed_resize_window_to_scaled_frame_size(
                    &app,
                    &(frame_width, frame_height),
                    (width, height),
                );
            }
        }
    }
}
