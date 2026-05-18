use super::window_cropping::CroppedArea;
use crate::{
    gpu_mirror_display::state::Application,
    ui_state::{ScaleDecision, UiState, VideoAspect, VideoLocation, WindowBehaviour},
};
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureTransformed {
    NoTextureTransform,
    TextureTransformedByVerts,
}

pub fn calculate_frame_transformations_for_settings(
    app: &Application,
    settings_state: &UiState,
    crop: &CroppedArea,
) -> (Vec<Vertex>, TextureTransformed) {
    let mut temp = match &settings_state.aspect_ratio {
        VideoAspect::MaintainAspectRatio(scale_decision, window_behaviour) => {
            match scale_decision {
                ScaleDecision::DontScale => {
                    (VERTICES.to_vec(), TextureTransformed::NoTextureTransform)
                }
                ScaleDecision::Scale => {
                    let pos = match window_behaviour {
                        WindowBehaviour::SizeMatchesMirrorAspect => VideoLocation::NorthWest,
                        WindowBehaviour::SizeSetByUser(pos) => pos.clone(),
                    };

                    (
                        position_centered_verts(
                            &center_verticies(
                                VERTICES.to_vec(),
                                (crop.size.width, crop.size.height),
                                app.systems.window.inner_size().into(),
                            ),
                            &pos,
                        ),
                        TextureTransformed::TextureTransformedByVerts,
                    )
                }
            }
        }
        VideoAspect::DoNotMaintainAspect => (
            (VERTICES.to_vec()),
            TextureTransformed::TextureTransformedByVerts,
        ),
    };

    // If texture matches the window size, it should never be transformed so that
    // it maintains it's pixel perfect output.
    let PhysicalSize { width, height } = app.systems.window.inner_size();

    if crop.size.width as u32 == width && crop.size.height <= height
        || crop.size.height as u32 == height && crop.size.width as u32 <= width
    {
        temp = (VERTICES.to_vec(), TextureTransformed::NoTextureTransform);

        // except when the aspect ratio isn't supposed to be maintanied.
        match &settings_state.aspect_ratio {
            VideoAspect::DoNotMaintainAspect => {
                if !(crop.size.width as u32 == width
                    && crop.size.height <= height
                    && crop.size.height as u32 == height
                    && crop.size.width as u32 <= width)
                {
                    temp = (
                        VERTICES.to_vec(),
                        TextureTransformed::TextureTransformedByVerts,
                    );
                }
            }
            _ => {}
        }
    }

    temp
}

pub fn center_verticies(
    with: Vec<Vertex>,
    window_sizes: (u32, u32),
    surface_size: (u32, u32),
) -> Vec<Vertex> {
    let window_sizes: (f32, f32) = (window_sizes.0 as f32, window_sizes.1 as f32);
    let surface_size: (f32, f32) = (surface_size.0 as f32, surface_size.1 as f32);

    let window_ratio: f32 = window_sizes.0 as f32 / (window_sizes.1 as f32);

    // More than > 1  means that it's a tower, like a skyscraper
    // Less than < 1 means it's a horizontal wall
    let surface_ratio: f32 = surface_size.0 as f32 / (surface_size.1 as f32);

    let to_fix_x;
    let to_fix_y;

    if window_ratio > surface_ratio {
        // The window is more like a wall fitting into a tower
        //
        // the width should stay the same, but the height needs to shrink. Since the width is larger on the window,
        // it should still touch both sides of the tower.
        to_fix_x = 1.0;
        to_fix_y = surface_ratio / window_ratio;
    } else {
        // The window is more like a tower fitting into a wall.
        to_fix_x = (1.0 / surface_ratio) * window_ratio;
        to_fix_y = 1.0;
    }

    with.into_iter()
        .map(|mut v| {
            v.position[0] *= to_fix_x;
            v.position[1] *= to_fix_y;

            v
        })
        .collect()
}

pub fn add_verticies_to_gpu_buffer(app: &mut Application, verticies: &Vec<Vertex>) {
    let vert_bytes = bytemuck::cast_slice(&verticies);

    let verticies = app
        .systems
        .wgpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: vert_bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });

    app.mirror.render.mirror_rendering.vertex_buffer = verticies;
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub tex_coords: [f32; 2],
}

pub const VERTICES: &[Vertex] = &[
    // Top left
    Vertex {
        position: [-1.0, 1.0, 0.0],
        tex_coords: [0.0, 1.0 - 1.0],
    },
    // Top right
    Vertex {
        position: [1.0, 1.0, 0.0],
        tex_coords: [1.0, 1.0 - 1.0],
    },
    // Bottom left
    Vertex {
        position: [-1.0, -1.0, 0.0],
        tex_coords: [0.0, 1.0 - 0.0],
    },
    // Bottom right
    Vertex {
        position: [1.0, -1.0, 0.0],
        tex_coords: [1.0, 1.0 - 0.0],
    },
];

pub fn position_centered_verts(verts: &[Vertex], to_position: &VideoLocation) -> Vec<Vertex> {
    let (left_x, top_y) = (verts[0].position[0], verts[0].position[1]);

    let x_shifted_by = left_x - (-1.0);
    let y_shifted_by = 1.0 - top_y;

    let (cx, cy) = match to_position {
        // add the shift back
        VideoLocation::NorthWest => (-x_shifted_by, y_shifted_by),
        // add only y back
        VideoLocation::North => (0.0, y_shifted_by),
        // add y back... shift x the opposite direction that it's supposed to normally go
        VideoLocation::NorthEast => (x_shifted_by, y_shifted_by),
        // add x back, y stays the same
        VideoLocation::West => (-x_shifted_by, 0.0),
        // no change, it's centered
        VideoLocation::Center => (0.0, 0.0),
        // x opposite way, y is no shift
        VideoLocation::East => (x_shifted_by, 0.0),
        // add x back, y is reversed
        VideoLocation::SouthWest => (-x_shifted_by, -y_shifted_by),
        // x has no shift, y is reversed
        VideoLocation::South => (0.0, -y_shifted_by),
        // x is reversed, y is reversed
        VideoLocation::SouthEast => (x_shifted_by, -y_shifted_by),
    };

    verts
        .iter()
        .map(|v| {
            let mut vertex = v.clone();

            vertex.position[0] += cx;
            vertex.position[1] += cy;
            vertex
        })
        .collect()
}
