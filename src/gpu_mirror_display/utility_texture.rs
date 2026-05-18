use super::window_cropping::CroppedArea;
use crate::{
    global_application_state::{FrameData, FrameLayout, LastReported},
    gpu_mirror_display::state::Application,
    ui_state::VideoLocation,
};
use smithay::backend::allocator::Buffer;
use std::sync::Arc;
use wgpu::{Extent3d, Origin3d, TexelCopyBufferLayout, TexelCopyTextureInfo};

pub(crate) fn crop_frame_to_origin<'a>(
    _frame: &Arc<LastReported>,
    overlay: &'a OverlayImage,
    cropped: &CroppedArea,
) -> PositioningData<'a> {
    // The cropped part is moved to the origin, this slices the frame data and offsets
    // the buffer to skip on the x-axis. The size of the cropped image is used
    // to avoid clipping the cropped data before positioning it
    let positioned_frame = position_image(
        &overlay,
        (
            (0 - (cropped.relative_to_frame_position.x as i32)),
            (0 - (cropped.relative_to_frame_position.y as i32)),
        ),
        (cropped.size.width, cropped.size.height),
    );

    positioned_frame
}

pub struct OverlayImage<'a> {
    pub data: DmaOrCpuMemory<'a>,
    pub dimensions: Extent3d,
    pub layout: TexelCopyBufferLayout,
}

pub(crate) fn define_frame<'a>(
    frame: &'a Arc<LastReported>,
    cropped: &'a CroppedArea,
) -> OverlayImage<'a> {
    let frame_data: (DmaOrCpuMemory<'a>, FrameLayout) = match &*frame.frame_data {
        FrameData::CpuData(cpu_frame) => {
            let temp = &(cpu_frame.frame_data);
            (DmaOrCpuMemory::Cpu(&temp), (&cpu_frame.layout).clone())
        }
        FrameData::DmaBuffers(dma_frame) => {
            let size = dma_frame.frame_data.size();

            let temp = FrameLayout {
                width: size.w as u32,
                height: size.h as u32,
                bytes_per_pixel: 4,
            };

            (DmaOrCpuMemory::Dma, temp)
        }
    };

    let (data, layout) = frame_data;

    let overlay = OverlayImage {
        data: data,
        dimensions: Extent3d {
            width: cropped.size.width + cropped.relative_to_frame_position.x,
            height: cropped.size.height + cropped.relative_to_frame_position.y,
            depth_or_array_layers: 1,
        },
        layout: TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(layout.bytes_per_pixel as u32 * layout.width),
            rows_per_image: Some(layout.height),
        },
    };

    overlay
}

pub enum DmaOrCpuMemory<'a> {
    Dma,
    Cpu(&'a [u8]),
}

pub(crate) struct PositioningData<'a> {
    pub origin: Origin3d,
    pub layout_after: TexelCopyBufferLayout,
    pub dimensions_after: Extent3d,
    pub data: DmaOrCpuMemory<'a>,
    // range: RangeFrom<usize>,
}

/// It's complicated to write an image that needs to be cut and positioned off
/// screen with WebGPU. This method just handles the cropping and positioning
/// back to screen coordinates.
pub(crate) fn write_image_to_texture(
    app: &Application,
    tex: &wgpu::Texture,
    img: &OverlayImage,
    position: (i32, i32),
) {
    let positoning = position_image(&img, (position.0, position.1), (tex.width(), tex.height()));

    let data = match img.data {
        DmaOrCpuMemory::Dma => {
            return;
        }
        DmaOrCpuMemory::Cpu(items) => items,
    };

    app.systems.wgpu.queue.write_texture(
        TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: positoning.origin,
            aspect: wgpu::TextureAspect::All,
        },
        &data,
        positoning.layout_after,
        positoning.dimensions_after,
    );
}

pub(crate) fn position_image<'a, 'b>(
    overlay: &'a OverlayImage<'b>,
    (mut x, mut y): (i32, i32),
    _dimensions @ (s_width, s_height): (u32, u32),
) -> PositioningData<'a> {
    let mut o_width: i32 = overlay.dimensions.width as i32;
    let mut o_height: i32 = overlay.dimensions.height as i32;
    let (mut skip_rows, mut skip_cols) = (0, 0);

    if x < 0 {
        o_width -= x.abs();
        skip_cols += x.abs();
        x = 0;

        if o_width < 0 {
            o_width = 0;
        }
    }

    if y < 0 {
        o_height -= y.abs();
        skip_rows += y.abs();
        y = 0;

        if o_height < 0 {
            o_height = 0;
        }
    }

    if x + o_width > s_width as i32 {
        let over_by_x = (x + o_width) - (s_width as i32);

        o_width -= over_by_x;

        if o_width < 0 {
            o_width = 0;
            x = 0;
        }
    }

    if y + o_height > s_height as i32 {
        let over_by_y = (y + o_height) - (s_height as i32);

        o_height -= over_by_y;

        if o_height < 0 {
            o_height = 0;
            y = 0;
        }
    }

    let item_dimensions = Extent3d {
        width: o_width as u32,
        height: o_height as u32,
        depth_or_array_layers: 1,
    };

    let layout = TexelCopyBufferLayout {
        offset: overlay.layout.offset + (skip_cols * 4) as u64,
        bytes_per_row: overlay.layout.bytes_per_row,
        rows_per_image: overlay.layout.rows_per_image,
    };

    let pos = Origin3d {
        x: x as u32,
        y: y as u32,
        z: 0,
    };

    let skip_rows: usize = skip_rows as usize;
    let bytes_per_row = overlay.layout.bytes_per_row.unwrap() as usize;

    let data = match &overlay.data {
        DmaOrCpuMemory::Dma => DmaOrCpuMemory::Dma,
        DmaOrCpuMemory::Cpu(items) => DmaOrCpuMemory::Cpu(&items[skip_rows * bytes_per_row..]),
    };

    let temp = PositioningData {
        origin: pos,
        layout_after: layout,
        dimensions_after: item_dimensions,
        data: data,
    };

    temp
}

pub fn calculate_window_position(
    &(texture_width, texture_height): &(i32, i32),
    &(window_width, window_height): &(i32, i32),
    location: &VideoLocation,
) -> (i32, i32) {
    match location {
        VideoLocation::NorthWest => (0, 0),
        VideoLocation::North => ((window_width / 2) - (texture_width / 2), 0),
        VideoLocation::NorthEast => (window_width - texture_width, 0),
        VideoLocation::West => (0, (window_height / 2) - (texture_height / 2)),
        VideoLocation::Center => (
            (window_width / 2) - (texture_width / 2),
            (window_height / 2) - (texture_height / 2),
        ),
        VideoLocation::East => (
            window_width - texture_width,
            (window_height / 2) - (texture_height / 2),
        ),
        VideoLocation::SouthWest => (0, window_height - texture_height),
        VideoLocation::South => (
            (window_width / 2) - (texture_width / 2),
            window_height - texture_height,
        ),
        VideoLocation::SouthEast => (window_width - texture_width, window_height - texture_height),
    }
}
