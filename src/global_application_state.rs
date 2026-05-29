use crate::{
    gpu_mirror_display::defaults::{FP_ID, PRESENT_PREFERENCES},
    ui_state::LoadedProfiles,
};
use detect_desktop_environment::DesktopEnvironment;
use lamco_wgpu::smithay_reexports::Dmabuf;
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
    time::SystemTime,
};

pub static FRAME_TRANSFER: LazyLock<Mutex<Option<Arc<LastReported>>>> =
    LazyLock::new(|| Mutex::new(None));

pub const SAFE_MODE: &'static str = "SAFE_MODE";
pub const FPS_TRACKING: &'static str = "FPS_TRACKING";

pub const VERSION: &'static str = "0.0.7";

pub const CONFIG_FOLDER: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut path =
        std::env::home_dir().unwrap_or(std::env::current_dir().unwrap_or("/home".into()));

    path.push(".var/app");
    path.push(FP_ID);
    path.push("config/fracture");

    path
});

pub const FRACTURE_PROFILE_FILENAME: &'static str = "profiles.json";

#[derive(Debug)]
pub enum ProfileLoadingErr {
    DirectoryCreation(std::io::Error),
    ReadErr(std::io::Error),
    InvalidFormat(serde_json::Error),
    CreateNew(std::io::Error),
    WriteErr(std::io::Error),
    EmptyList,
}

pub fn profiles_filepath() -> PathBuf {
    let mut path = CONFIG_FOLDER.clone();

    path.push(FRACTURE_PROFILE_FILENAME);

    path
}

#[derive(Debug)]
pub enum ProfileSavingErr {
    FileCreationErr(ProfileLoadingErr),
    Serialization(serde_json::Error),
    FileOpenErr(std::io::Error),
    FileWriteErr(std::io::Error),
}

#[derive(Debug)]
pub enum ProfileResetErr {
    FileDeletionErr(std::io::Error),
    SaveErr(ProfileSavingErr),
}

pub fn reset_profiles(from_loaded: LoadedProfiles) -> Result<(), ProfileResetErr> {
    let path = profiles_filepath();

    let res: Result<(), std::io::Error> = std::fs::remove_file(&path);

    match res {
        Ok(_) => match save_profiles(from_loaded) {
            Ok(_) => Ok(()),
            Err(e) => Err(ProfileResetErr::SaveErr(e)),
        },
        Err(e) => Err(ProfileResetErr::FileDeletionErr(e)),
    }
}

pub fn save_profiles(mut from_loaded: LoadedProfiles) -> Result<(), ProfileSavingErr> {
    let file_verification = load_profiles();

    match file_verification {
        Ok(_) => {
            if from_loaded.profiles.len() == 0 {
                from_loaded.profiles = vec![Default::default()];
            }

            let path = profiles_filepath();

            match std::fs::File::create(&path) {
                Ok(mut file) => {
                    let serde = serde_json::to_string_pretty(&from_loaded);

                    if serde.is_err() {
                        return Err(ProfileSavingErr::Serialization(serde.unwrap_err()));
                    }

                    let serde = serde.unwrap();

                    match file.write_all(&serde.as_bytes()) {
                        Ok(_) => return Ok(()),
                        Err(e) => Err(ProfileSavingErr::FileWriteErr(e)),
                    }
                }
                Err(e) => Err(ProfileSavingErr::FileOpenErr(e)),
            }
        }
        Err(e) => Err(ProfileSavingErr::FileCreationErr(e)),
    }
}

pub fn load_profiles() -> Result<LoadedProfiles, ProfileLoadingErr> {
    let mut path = CONFIG_FOLDER.clone();

    let res = std::fs::create_dir_all(&path);

    let Ok(()) = res else {
        return Err(ProfileLoadingErr::DirectoryCreation(res.unwrap_err()));
    };

    path = profiles_filepath();

    match std::fs::File::open(&path) {
        Ok(mut file) => {
            let mut data = String::new();

            let res: Result<usize, std::io::Error> = file.read_to_string(&mut data);

            let Ok(_) = res else {
                return Err(ProfileLoadingErr::ReadErr(res.unwrap_err()));
            };

            let res: Result<LoadedProfiles, serde_json::Error> = serde_json::from_str(&data);

            let Ok(val) = res else {
                return Err(ProfileLoadingErr::InvalidFormat(res.unwrap_err()));
            };

            if val.list().len() == 0 {
                return Err(ProfileLoadingErr::EmptyList);
            }

            return Ok(val);
        }

        Err(_) => {
            let res: Result<std::fs::File, std::io::Error> = std::fs::File::create_new(&path);

            let Ok(mut file) = res else {
                return Err(ProfileLoadingErr::CreateNew(res.unwrap_err()));
            };

            let cfg = LoadedProfiles::default();

            let contents = serde_json::to_string_pretty(&cfg).unwrap();

            let res: Result<(), std::io::Error> = file.write_all(&contents.as_bytes());

            let Ok(_) = res else {
                return Err(ProfileLoadingErr::WriteErr(res.unwrap_err()));
            };

            return Ok(cfg);
        }
    }
}

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
