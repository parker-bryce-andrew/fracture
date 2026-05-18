use detect_desktop_environment::DesktopEnvironment;
use lamco_wgpu::smithay_reexports::Dmabuf;
use std::{
    sync::{Arc, LazyLock, Mutex},
    time::SystemTime,
};

use crate::gpu_mirror_display::defaults::PRESENT_PREFERENCES;

pub static FRAME_TRANSFER: LazyLock<Mutex<Option<Arc<LastReported>>>> =
    LazyLock::new(|| Mutex::new(None));

pub const SAFE_MODE: &'static str = "SAFE_MODE";

pub const VERSION: &'static str = "0.0.5";

pub static FOUND_VERSION: LazyLock<String> = LazyLock::new(|| {
    if let Ok(v) = reqwest::blocking::get("https://fracture.systems/fracture/VERSION") {
        if let Ok(v) = v.text() {
            return v.trim().into();
        }
    }

    VERSION.into()
});

pub static DESKTOP_ENV_IS_GNOME: LazyLock<bool> =
    LazyLock::new(|| match DesktopEnvironment::detect() {
        Some(de) => match de {
            DesktopEnvironment::Gnome => true,

            other => {
                println!("{other:?}");

                false
            }
        },
        None => false,
    });

pub static AVAILABLE_PRESETS: LazyLock<Mutex<Vec<wgpu::PresentMode>>> =
    LazyLock::new(|| Mutex::new(PRESENT_PREFERENCES.to_vec()));

/*

// todo: Add saved defaults.

static DEFUALT_SETTINGS_FILE_PATH: &'static str = "~/.config/default.json";

pub static DEFAULT_SETTINGS: LazyLock<SetUiState> = LazyLock::new(|| {
    if let Ok(mut file) = std::fs::File::open(DEFUALT_SETTINGS_FILE_PATH) {
        let mut text = String::new();

        if let Ok(_) = file.read_to_string(&mut text) {
            let state: Result<SetUiState, _> = serde_json::de::from_str(&text);

            if let Ok(state) = state {
                return state;
            }
        }
    }

    CreateUiState::default().into()
}); */

#[derive(Clone, Debug)]
pub struct FrameLayout {
    pub width: u32,
    pub height: u32,
    pub bytes_per_pixel: u8,
}

impl FrameLayout {
    #[allow(unused)]
    fn bytes_per_row(&self) -> u32 {
        self.bytes_per_pixel as u32 * self.width
    }

    #[allow(unused)]
    fn size(&self) -> u32 {
        self.bytes_per_row() * self.height
    }
}
#[derive(Clone, Debug)]
pub struct CpuFrame {
    pub frame_data: Vec<u8>,
    pub layout: FrameLayout,
    pub scan_time: SystemTime,
}

#[derive(Clone, Debug)]

pub struct DmaFrame {
    pub frame_data: Dmabuf,
    /// This is not a CPU memory copy of this DmaBuffer, but a memory
    /// copy of the last buffer that was copied to CPU memory. It
    /// can be this DmaBuffer, but it's likely to be an older DmaBuffer
    pub saved_cpu_frame: Option<Arc<CpuFrame>>,
}

#[derive(Clone, Debug)]
pub enum FrameData {
    CpuData(CpuFrame),
    DmaBuffers(DmaFrame),
}

#[derive(Clone, Debug)]
pub struct LastReported {
    pub frame_data: Arc<FrameData>,
    pub window_dimensions: (u32, u32),
    pub last_known_offsets: (u32, u32),
}
