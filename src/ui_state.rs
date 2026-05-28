use std::time::SystemTime;

use crate::{
    global_application_state::AVAILABLE_PRESETS,
    gpu_mirror_display::{defaults::CROP_COLOR, postprocessing_shaders::PostprocessingErrors},
};
use serde::{Deserialize, Serialize};
use wgpu::PresentMode;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum VideoLocation {
    NorthWest = 0,
    North = 1,
    NorthEast = 2,
    West = 3,
    Center = 4,
    East = 5,
    SouthWest = 6,
    South = 7,
    SouthEast = 8,
}

impl Into<VideoLocation> for i32 {
    fn into(self) -> VideoLocation {
        match self {
            0 => VideoLocation::NorthWest,
            1 => VideoLocation::North,
            2 => VideoLocation::NorthEast,
            3 => VideoLocation::West,
            4 => VideoLocation::Center,
            5 => VideoLocation::East,
            6 => VideoLocation::SouthWest,
            7 => VideoLocation::South,
            8 => VideoLocation::SouthEast,
            _ => VideoLocation::Center,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum WindowBackground {
    Transparent,
    Color(f32, f32, f32, f32),
}

impl Default for WindowBackground {
    fn default() -> Self {
        WindowBackground::Color(CROP_COLOR.0, CROP_COLOR.1, CROP_COLOR.2, CROP_COLOR.3)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum WindowBehaviour {
    SizeMatchesMirrorAspect,
    SizeSetByUser(VideoLocation),
}

impl Default for WindowBehaviour {
    fn default() -> Self {
        WindowBehaviour::SizeSetByUser(VideoLocation::Center)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]

pub enum ScaleDecision {
    DontScale,
    Scale,
}

impl Default for ScaleDecision {
    fn default() -> Self {
        ScaleDecision::Scale
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum VideoAspect {
    MaintainAspectRatio(ScaleDecision, WindowBehaviour),
    DoNotMaintainAspect,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TitleBarDisplay {
    HiddenTitleBar,
    TitleBarVisible,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RemoveColors {
    pub base_color: (f32, f32, f32),
    pub sensitivity: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GreenScreen {
    None,
    Color(RemoveColors),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Postprocessor {
    pub submitted_postprocessor: Option<String>,

    #[serde(skip)]
    pub editing_postprocessor: String,
    #[serde(skip)]
    pub last_errors: Option<PostprocessingErrors>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct AdjCopy {
    pub value: f64,
    pub lower: f64,
    pub upper: f64,
    pub step_increment: f64,
    pub page_increment: f64,
    pub page_size: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum WindowInteractions {
    Interactable,
    PassThrough,
}

pub struct AppConfiguration {
    pub active: UiState,
    pub saved: CreateUiState,
    pub profiles: LoadedProfiles,
}

#[derive(Clone, Debug)]
pub struct UiState {
    pub display_title: TitleBarDisplay,
    pub aspect_ratio: VideoAspect,
    pub frame_transparency: f32,
    pub green_screen: GreenScreen,
    pub postprocessor: Option<Postprocessor>,
    pub background: WindowBackground,
    pub need_rebuild: bool,
    pub updated: bool,
    // pub open_settings_ui: Option<bool>,
    pub gpu_requested_compile: bool,
    pub scroll_value: Option<AdjCopy>,
    pub magnify_filter: wgpu::FilterMode,
    pub minify_filter: wgpu::FilterMode,
    pub should_define_new_primary_sampler: bool,
    pub window_interactions: WindowInteractions,
    pub preset: wgpu::PresentMode,
    pub should_define_new_preset: bool,
    pub active_profile: usize,
    pub reload_profiles: bool,
    pub delayed_uptime_timer: Option<SystemTime>,
}

impl UiState {
    /// Transforms into a more readable version of the settings. This loses intricate
    /// details on the full settings state, but it's OK because the full state is mostly
    /// just managing a reactive state.
    ///
    /// The reactive state can mostly be reconstructed, but it's expensive to reconstruct it.
    /// The only time it's expected that a reconstruction will happen is when settings are imported
    /// from a JSON.
    pub fn lossy_into_set_ui(&self) -> SetUiState {
        let temp: UiState = self.clone();

        let temp = SetUiState {
            display_title: temp.display_title,
            aspect_ratio: temp.aspect_ratio,
            frame_transparency: temp.frame_transparency,
            green_screen: temp.green_screen,
            postprocessor: temp.postprocessor,
            window_background: temp.background,
            magnify_filter: temp.magnify_filter,
            minify_filter: temp.minify_filter,
            window_interactions: temp.window_interactions,
            present: temp.preset,
        };

        temp
    }
}

/// This is just used for display purposes. The actual full settings state
/// has extra fields that are intended for managing the state of the displayed
/// user interface. They serve no configurable purpose, and are not useful
/// in configuring the application.
///
/// When the final debug is provided to the user of the settings user interface,
/// these fields are set from the more complicated user settings tracker
/// then displayed to the user using the entire struct with debug printing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetUiState {
    pub display_title: TitleBarDisplay,
    pub aspect_ratio: VideoAspect,
    pub frame_transparency: f32,
    pub green_screen: GreenScreen,
    pub window_background: WindowBackground,
    pub postprocessor: Option<Postprocessor>,
    pub magnify_filter: wgpu::FilterMode,
    pub minify_filter: wgpu::FilterMode,
    pub window_interactions: WindowInteractions,
    pub present: PresentMode,
}

impl Default for SetUiState {
    fn default() -> Self {
        let temp = CreateUiState::default();
        let temp: UiState = temp.into();
        temp.lossy_into_set_ui()
    }
}

impl SetUiState {
    /// This creates a settings state that can be used by the application, but the transform
    /// process into SetUiState from UiState is lossy, and the original state is lost. This will build a new state
    /// that works with the application, but using the new state will trigger defining a new render pipeline,
    /// recompiling shaders, etc. This is to say, it's expensive to run the state generated from here, but most
    /// users won't notice because it's only expected to happen when users are using a pre-saved export
    /// on importing.
    pub fn build_new_full_settings_state(&self) -> UiState {
        let temp = self.clone();

        let SetUiState {
            display_title,
            aspect_ratio,
            frame_transparency,
            green_screen,
            window_background,
            postprocessor,
            magnify_filter,
            minify_filter,
            window_interactions,
            present: preset,
        } = temp;

        let selected_preset;

        let available_presets = { AVAILABLE_PRESETS.lock().unwrap().clone() };

        if available_presets.contains(&preset) {
            selected_preset = preset;
        } else {
            selected_preset = available_presets[0];
        }

        let mut temp = UiState {
            display_title,
            aspect_ratio,
            frame_transparency,
            green_screen,
            postprocessor,
            background: window_background,
            need_rebuild: true,
            updated: true,
            // open_settings_ui: None,
            gpu_requested_compile: true,
            scroll_value: None,
            magnify_filter,
            minify_filter,
            should_define_new_primary_sampler: true,
            window_interactions,
            preset: selected_preset,
            should_define_new_preset: true,
            active_profile: 0,
            reload_profiles: true,
            delayed_uptime_timer: None,
        };

        if let Some(postprocessor) = &mut temp.postprocessor {
            if let Some(submission) = &postprocessor.submitted_postprocessor {
                postprocessor.editing_postprocessor = submission.clone();
            }
        }

        temp
    }

    /// This is just a helper method to avoid having to use serde_json directly.
    pub fn json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

pub const DEFAULT_MAGNIFY_FILTER: wgpu::FilterMode = wgpu::FilterMode::Linear;
pub const DEFAULT_MINIFY_FILTER: wgpu::FilterMode = wgpu::FilterMode::Linear;

impl Default for UiState {
    fn default() -> Self {
        Self {
            display_title: TitleBarDisplay::HiddenTitleBar,
            aspect_ratio: VideoAspect::MaintainAspectRatio(
                ScaleDecision::Scale,
                WindowBehaviour::SizeMatchesMirrorAspect,
            ),
            need_rebuild: false,
            updated: false,
            frame_transparency: 100.0,
            // open_settings_ui: None,
            green_screen: GreenScreen::None,
            postprocessor: None,
            gpu_requested_compile: true,
            scroll_value: None,
            background: WindowBackground::Transparent,
            magnify_filter: DEFAULT_MAGNIFY_FILTER,
            minify_filter: DEFAULT_MINIFY_FILTER,
            should_define_new_primary_sampler: true,
            window_interactions: WindowInteractions::Interactable,
            preset: *&AVAILABLE_PRESETS.lock().unwrap()[0].clone(),
            should_define_new_preset: true,
            active_profile: 0,
            reload_profiles: true,
            delayed_uptime_timer: None,
        }
    }
}

impl UiState {
    fn on_all_changes(&mut self) -> &mut Self {
        self.updated = true;

        self
    }

    pub fn update(&mut self) -> &mut Self {
        self.need_rebuild = true;

        self.on_all_changes()
    }

    pub fn update_delayed_rebuild(&mut self) -> &mut Self {
        self.delayed_uptime_timer = Some(SystemTime::now());

        self.on_all_changes()
    }
}

/// See the definition for SetUiState first.
///
/// Using this is just about perfection in defining a version. The other has debug fields
/// related to the UI. This one is for programmatically using with an IDE
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CreateUiState {
    pub display_title: Option<TitleBarDisplay>,
    pub aspect_ratio: Option<VideoAspect>,
    pub frame_transparency: Option<f32>,
    pub green_screen: Option<GreenScreen>,
    pub window_background: Option<WindowBackground>,
    pub postprocessor: Option<String>,
    pub magnify_filter: Option<wgpu::FilterMode>,
    pub minify_filter: Option<wgpu::FilterMode>,
    pub window_interactions: Option<WindowInteractions>,
    pub present: Option<PresentMode>,
}

impl Default for CreateUiState {
    fn default() -> Self {
        let UiState {
            display_title,
            aspect_ratio,
            frame_transparency,
            green_screen,
            postprocessor,
            window_interactions,
            background,
            need_rebuild: _,
            updated: _,
            // open_settings_ui: _,
            gpu_requested_compile: _,
            scroll_value: _,
            magnify_filter,
            minify_filter,
            should_define_new_primary_sampler: _,
            preset: presets,
            should_define_new_preset: _,
            active_profile: _,
            reload_profiles: _,
            delayed_uptime_timer: _,
        } = UiState::default();

        Self {
            display_title: Some(display_title),
            aspect_ratio: Some(aspect_ratio),
            frame_transparency: Some(frame_transparency),
            green_screen: Some(green_screen),
            window_background: Some(background),
            postprocessor: postprocessor
                .map(|v| v.submitted_postprocessor)
                .unwrap_or(None),
            magnify_filter: Some(magnify_filter),
            minify_filter: Some(minify_filter),
            window_interactions: Some(window_interactions),
            present: Some(presets),
        }
    }
}

impl Into<SetUiState> for CreateUiState {
    fn into(self) -> SetUiState {
        let CreateUiState {
            display_title,
            aspect_ratio,
            frame_transparency,
            green_screen,
            window_background,
            postprocessor,
            magnify_filter,
            minify_filter,
            window_interactions,
            present: presets,
        } = self;

        SetUiState {
            display_title: display_title.unwrap_or(UiState::default().display_title),
            aspect_ratio: aspect_ratio.unwrap_or(UiState::default().aspect_ratio),
            frame_transparency: frame_transparency.unwrap_or(UiState::default().frame_transparency),
            green_screen: green_screen.unwrap_or(UiState::default().green_screen),
            window_background: window_background.unwrap_or(UiState::default().background),
            postprocessor: postprocessor.map(|v| Postprocessor {
                submitted_postprocessor: Some(v.clone()),
                editing_postprocessor: v.clone(),
                last_errors: None,
            }),
            magnify_filter: magnify_filter.unwrap_or(UiState::default().magnify_filter),
            minify_filter: minify_filter.unwrap_or(UiState::default().minify_filter),
            window_interactions: window_interactions
                .unwrap_or(UiState::default().window_interactions),
            present: presets.unwrap_or(UiState::default().preset),
        }
    }
}

impl Into<UiState> for CreateUiState {
    fn into(self) -> UiState {
        let temp: SetUiState = self.into();

        temp.build_new_full_settings_state()
    }
}

impl Into<CreateUiState> for SetUiState {
    fn into(self) -> CreateUiState {
        let temp: SetUiState = self;

        CreateUiState {
            display_title: Some(temp.display_title),
            aspect_ratio: Some(temp.aspect_ratio),
            frame_transparency: Some(temp.frame_transparency),
            green_screen: Some(temp.green_screen),
            window_background: Some(temp.window_background),
            postprocessor: temp
                .postprocessor
                .map(|v| v.submitted_postprocessor)
                .unwrap_or(None),
            magnify_filter: Some(temp.magnify_filter),
            minify_filter: Some(temp.minify_filter),
            window_interactions: Some(temp.window_interactions),
            present: Some(temp.present),
        }
    }
}

impl Into<UiState> for SetUiState {
    fn into(self) -> UiState {
        self.build_new_full_settings_state()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedProfiles {
    pub profiles: Vec<Profile>,
}

impl Default for LoadedProfiles {
    fn default() -> Self {
        Self {
            profiles: vec![Profile::default()],
        }
    }
}

impl LoadedProfiles {
    pub fn user_default(&self) -> Profile {
        self.profiles[0].clone()
    }

    pub fn list(&self) -> &Vec<Profile> {
        &self.profiles
    }

    /// Returns the value at a specified index. This wraps the index if it is larger than the list.
    pub fn get(&self, idx: usize) -> &Profile {
        &self.profiles[idx % self.profiles.len()]
    }
}

impl Default for Profile {
    fn default() -> Self {
        let cfg: CreateUiState = Default::default();

        // cfg.present = None;

        Self {
            name: Some("New Profile".into()),
            config: cfg,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: Option<String>,
    pub config: CreateUiState,
}
