use crate::gpu_mirror_display::utility_texture::DmaOrCpuMemory;

use super::utility_texture::OverlayImage;
use wgpu::{Extent3d, TexelCopyBufferLayout};

pub const ICON_SELECT_SCREEN_AREA: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/1.bin")),
    dimensions: Extent3d {
        width: 500,
        height: 500,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(500 * 4),
        rows_per_image: Some(500),
    },
};

pub const ICON_EXIT_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/2.bin")),
    dimensions: Extent3d {
        width: 25,
        height: 25,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(25 * 4),
        rows_per_image: Some(25),
    },
};

pub const ICON_EXIT_NO_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/3.bin")),
    dimensions: Extent3d {
        width: 25,
        height: 25,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(25 * 4),
        rows_per_image: Some(25),
    },
};

pub const ICON_SQUARE_NO_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/4.bin")),
    dimensions: Extent3d {
        width: 25,
        height: 25,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(25 * 4),
        rows_per_image: Some(25),
    },
};

pub const ICON_SQUARE_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/5.bin")),
    dimensions: Extent3d {
        width: 25,
        height: 25,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(25 * 4),
        rows_per_image: Some(25),
    },
};

pub const ICON_MINIMIZE_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/6.bin")),
    dimensions: Extent3d {
        width: 25,
        height: 25,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(25 * 4),
        rows_per_image: Some(25),
    },
};

pub const ICON_MINIMIZE_NO_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/7.bin")),
    dimensions: Extent3d {
        width: 25,
        height: 25,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(25 * 4),
        rows_per_image: Some(25),
    },
};

pub const ICON_GEAR_NO_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/8.bin")),
    dimensions: Extent3d {
        width: 50,
        height: 50,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(50 * 4),
        rows_per_image: Some(50),
    },
};

pub const ICON_GEAR_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/9.bin")),
    dimensions: Extent3d {
        width: 50,
        height: 50,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(50 * 4),
        rows_per_image: Some(50),
    },
};

pub const ICON_PIP_NO_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/10.bin")),
    dimensions: Extent3d {
        width: 50,
        height: 50,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(50 * 4),
        rows_per_image: Some(50),
    },
};

pub const ICON_PIP_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/11.bin")),
    dimensions: Extent3d {
        width: 50,
        height: 50,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(50 * 4),
        rows_per_image: Some(50),
    },
};

pub const ICON_TRASH_NO_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/12.bin")),
    dimensions: Extent3d {
        width: 50,
        height: 50,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(50 * 4),
        rows_per_image: Some(50),
    },
};

pub const ICON_TRASH_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/13.bin")),
    dimensions: Extent3d {
        width: 50,
        height: 50,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(50 * 4),
        rows_per_image: Some(50),
    },
};

pub const ICON_DIAMOND_PROFILE_NO_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/14.bin")),
    dimensions: Extent3d {
        width: 50,
        height: 50,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(50 * 4),
        rows_per_image: Some(50),
    },
};

pub const ICON_DIAMOND_PROFILE_FILL: OverlayImage = OverlayImage {
    data: DmaOrCpuMemory::Cpu(include_bytes!("../../img/bin/15.bin")),
    dimensions: Extent3d {
        width: 50,
        height: 50,
        depth_or_array_layers: 1,
    },
    layout: TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(50 * 4),
        rows_per_image: Some(50),
    },
};
