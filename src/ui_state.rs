use crate::{
    global_application_state::AVAILABLE_PRESETS,
    gpu_mirror_display::{defaults::CROP_COLOR, postprocessing_shaders::PostprocessingErrors},
};
use serde::{Deserialize, Serialize};
use wgpu::PresentMode;

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WindowBackground {
    Transparent,
    Color(f32, f32, f32, f32),
}

impl Default for WindowBackground {
    fn default() -> Self {
        WindowBackground::Color(CROP_COLOR.0, CROP_COLOR.1, CROP_COLOR.2, CROP_COLOR.3)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WindowBehaviour {
    SizeMatchesMirrorAspect,
    SizeSetByUser(VideoLocation),
}

impl Default for WindowBehaviour {
    fn default() -> Self {
        WindowBehaviour::SizeSetByUser(VideoLocation::Center)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]

pub enum ScaleDecision {
    DontScale,
    Scale,
}

impl Default for ScaleDecision {
    fn default() -> Self {
        ScaleDecision::Scale
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WindowInteractions {
    Interactable,
    PassThrough,
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
            display_title: TitleBarDisplay::TitleBarVisible,
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
        }
    }
}

impl UiState {
    pub fn update(&mut self) -> &mut Self {
        self.need_rebuild = true;
        self.update_no_rebuild()
    }

    pub fn update_no_rebuild(&mut self) -> &mut Self {
        self.updated = true;
        self
    }
}

/// See the definition for SetUiState first.
///
/// Using this is just about perfection in defining a version. The other has debug fields
/// related to the UI. This one is for programmatically using with an IDE
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateUiState {
    pub display_title: TitleBarDisplay,
    pub aspect_ratio: VideoAspect,
    pub frame_transparency: f32,
    pub green_screen: GreenScreen,
    pub window_background: WindowBackground,
    pub postprocessor: Option<String>,
    pub magnify_filter: wgpu::FilterMode,
    pub minify_filter: wgpu::FilterMode,
    pub window_interactions: WindowInteractions,
    pub preset: PresentMode,
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
        } = UiState::default();

        Self {
            display_title,
            aspect_ratio,
            frame_transparency,
            green_screen,
            window_background: background,
            postprocessor: postprocessor
                .map(|v| v.submitted_postprocessor)
                .unwrap_or(None),
            magnify_filter,
            minify_filter,
            window_interactions: window_interactions,
            preset: presets,
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
            preset: presets,
        } = self;

        SetUiState {
            display_title,
            aspect_ratio,
            frame_transparency,
            green_screen,
            window_background,
            postprocessor: postprocessor.map(|v| Postprocessor {
                submitted_postprocessor: Some(v.clone()),
                editing_postprocessor: v.clone(),
                last_errors: None,
            }),
            magnify_filter,
            minify_filter,
            window_interactions: window_interactions,
            present: presets,
        }
    }
}

impl Into<UiState> for CreateUiState {
    fn into(self) -> UiState {
        let temp: SetUiState = self.into();

        temp.build_new_full_settings_state()
    }
}

impl Into<UiState> for SetUiState {
    fn into(self) -> UiState {
        self.build_new_full_settings_state()
    }
}
