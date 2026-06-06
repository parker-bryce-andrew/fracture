use super::{
    input::events_mouse::ResizeInteractionsState, overlay_ui::UiFlag, window_cropping::CroppedArea,
};
use crate::{
    application_channel_creator::GpuChannelSide,
    gpu_mirror_display::{event_loop::WrappedBridge, pipeline_definitions::SelectedGpuCaps},
    gtk_user_interfaces::settings_ui::SETTINGS_IS_RUNNING,
    stream_creation::utility_gnome_video_frame::PredictedWgpuFrameFormat,
    ui_state::{
        AppConfiguration, GreenScreen, TitleBarDisplay, UiState, VideoAspect, WindowBehaviour,
    },
};
use std::{
    sync::{Arc, mpsc::SendError},
    time::{Duration, SystemTime},
};
use wgpu::{PipelineLayout, PresentMode};
use winit::{dpi::PhysicalSize, window::Window};

#[derive(Debug, Clone)]
pub struct DmaStartupChecks {
    pub is_complete: bool,
    pub is_fail: bool,
    pub frames_checked: u32,
    pub frames_without_data: std::sync::Arc<std::sync::Mutex<u32>>,
    pub frames_with_data: std::sync::Arc<std::sync::Mutex<u32>>,
    pub dma_error_count: u32,
    pub fail_at: u32,
}

/// **Not used yet**
pub struct SettingsGtk;

pub struct UserInteractionState {
    pub mouse_clicks: Vec<((u32, u32), SystemTime)>,
    pub mouse_downs: Vec<((u32, u32), SystemTime)>,
    pub mouse_over_screen: bool,
    pub mouse_is_down: bool,
    pub mouse_select_start: (u32, u32),
    pub mouse_resize_state: ResizeInteractionsState,
    pub shift_left_is_down: bool,
    pub shift_right_is_down: bool,
}

pub struct UiRendering {
    pub ui_rendering_pipeline: wgpu::RenderPipeline,
    pub vertex_buffer2: wgpu::Buffer,
}

pub struct MirrorRendering {
    pub pipeline_layout: Option<PipelineLayout>,
    pub mirror_output_rendering_pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub mirror_fractured_texture: wgpu::Texture,
}

pub struct SharedRender {
    pub index_buffer: wgpu::Buffer,
    pub bindings: wgpu::BindGroup,
    pub used_video_format: PredictedWgpuFrameFormat,
    pub wrapping_render_count: u32,
    pub available_presents: Vec<PresentMode>,
    pub default_selected_capabilities: SelectedGpuCaps,
    pub diffuse_sampler: Option<wgpu::Sampler>,
    pub ui_flags: Option<wgpu::Buffer>,
    pub texture_bind_group_layout: Option<wgpu::BindGroupLayout>,
}

pub struct DetectedCapabilities {
    pub available_presents: Vec<PresentMode>,
    pub default_selected_capabilities: SelectedGpuCaps,
}

pub struct MirrorRenderer {
    pub ui_rendering: UiRendering,
    pub mirror_rendering: MirrorRendering,
    pub shared_rendering: SharedRender,
}

pub struct Mirror {
    pub render: MirrorRenderer,
}

pub struct WgpuContainer {
    pub bridge: WrappedBridge,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub surface: wgpu::Surface<'static>,
}
pub struct AppSystems {
    pub wgpu: WgpuContainer,
    pub window: std::sync::Arc<Window>,
    pub async_rt: Option<tokio::runtime::Runtime>,
}

/// **Not used yet**
pub struct PipewireController;

pub struct ExternalControl {
    pub channels: Arc<GpuChannelSide>,
    pub settings_ui: SettingsGtk,
    /// **Not used yet**
    pub pipewire: PipewireController,
}

pub struct InitState {
    pub dma_startup_checks: DmaStartupChecks,
    pub first_dma_sent: bool,
    pub track_fps: bool,
}

pub struct PreviousIteration {
    pub last_fracture_display_origin: wgpu::Origin3d,
    pub last_fracture_dimensions: wgpu::Extent3d,
    pub last_reported_offsets: (u32, u32),
    pub last_surface_size: PhysicalSize<u32>,
    pub last_frame_size: (u32, u32),
    pub last_known_mouse_position: (u32, u32),
}

pub struct IntricateState {
    pub crop_button_pressed: bool,
    pub in_crop_selection: bool,
    pub keep_borders: bool,
    pub resize_countdown_from_new_settings: i32,
    pub resize_countdown_started: bool,
    pub should_shutdown: bool,
    pub active_present: PresentMode,
    pub new_settings: bool,
}

#[derive(Debug, Clone)]
pub struct FpsReport {
    pub fps: f32,
    pub interval: Duration,
    pub draws: u32,
}

#[derive(Clone, Debug)]
pub enum FpsTrackerOrigin {
    WebGpu,
    Pipewire,
}

/// It's only used with an environmental varaible. It's not good.
///
/// todo: rewrite
pub struct FpsTracker {
    origin: Option<FpsTrackerOrigin>,
    start_time: SystemTime,
    /// For each draw, it takes 12 bytes of memory.
    ///
    /// 12 bytes * 120 FPS * 86,400 (24 hours) =  124,416,000 bytes
    ///
    /// 1 MB ~= 1 million bytes
    ///
    /// In 24 hours it takes 125 MB of memory.
    draws: Vec<SystemTime>,
    tracking: Vec<Duration>,
}

#[derive(Debug, Clone)]
pub struct FpsTrackerReport {
    #[allow(unused)]
    origin: Option<FpsTrackerOrigin>,
    #[allow(unused)]
    time: SystemTime,
    #[allow(unused)]
    timestamps: usize,
    #[allow(unused)]
    report: Vec<FpsReport>,
}

impl FpsTracker {
    pub fn new(origin: Option<FpsTrackerOrigin>, track: Option<Vec<Duration>>) -> FpsTracker {
        let mut tracking;

        if let Some(v) = track {
            tracking = v;
            tracking.sort();
        } else {
            tracking = vec![];
        }

        FpsTracker {
            start_time: SystemTime::now(),
            draws: vec![],
            tracking,
            origin: origin,
        }
    }

    pub fn increment(&mut self) {
        self.draws.push(SystemTime::now());
    }

    pub fn add_tracker(&mut self, interval: Duration) {
        self.tracking.push(interval);
        self.tracking.sort();
    }

    pub fn remove_tracker(&mut self, interval: Duration) {
        match self.tracking.binary_search_by_key(&interval, |idk| *idk) {
            Ok(idx) => {
                self.tracking.remove(idx);
            }
            Err(_) => {}
        }
    }

    pub fn fps(&self, interval: Duration) -> FpsReport {
        let res = self
            .draws
            .binary_search_by(|a| match a.elapsed().unwrap().cmp(&interval) {
                std::cmp::Ordering::Less => std::cmp::Ordering::Greater,
                std::cmp::Ordering::Equal => std::cmp::Ordering::Equal,
                std::cmp::Ordering::Greater => std::cmp::Ordering::Less,
            });

        let idx = match res {
            Ok(idx) => idx,
            Err(idx) => idx,
        };

        let mut duration = interval;

        if idx == 0 {
            duration = self.start_time.elapsed().unwrap();
        }

        let calls = (self.draws.len() - idx) as u32;

        FpsReport {
            fps: calls as f32 / duration.as_secs_f32(),
            interval: duration,
            draws: calls,
        }
    }

    pub fn report(&self) -> FpsTrackerReport {
        let mut temp = vec![];

        for v in &self.tracking {
            if v <= &self.start_time.elapsed().unwrap() {
                temp.push(self.fps(v.clone()));
            }
        }

        temp.push(self.fps(self.start_time.elapsed().unwrap()));

        FpsTrackerReport {
            time: SystemTime::now(),
            timestamps: self.draws.len(),
            report: temp,
            origin: self.origin.clone(),
        }
    }
}

pub struct AppStatistics {
    pub fps_tracking: FpsTracker,
}

pub struct AppState {
    pub cropped: Option<CroppedArea>,
    pub initialization_checks: InitState,
    pub last_iteration: PreviousIteration,
    pub intricate_todo_refactor: IntricateState,
    /// **Not used yet**
    pub current_activity: EnumeratedState,
}

pub struct Application {
    pub app_state: AppState,
    pub user_interaction: UserInteractionState,
    pub mirror: Mirror,
    pub configuration: AppConfiguration,
    pub metrics: AppStatistics,
    pub systems: AppSystems,
    pub external: ExternalControl,
}

/// **Not used yet**
pub struct SelectStart {
    pub x: u32,
    pub y: u32,
}

/// **Not used yet**
pub enum EnumeratedState {
    WaitingForSelection,
    InSelection(SelectStart),
    UsingSelection(CroppedArea),
}

impl SettingsGtk {
    /// Even when reporting Ok(()), it can seem like it failed if it immediately opens again.
    pub fn gtk_shutdown_signal(&self, app: &Application) -> Result<(), ShutdownSettingsErr> {
        let before = app.configuration.active.clone();

        let res = app.external.channels.gpu_sender_request.send(before);

        if let Err(e) = res {
            return Err(ShutdownSettingsErr::SendStateErr(e));
        }

        let res = app.external.channels.kill_gtk.send(());

        if let Err(e) = res {
            return Err(ShutdownSettingsErr::SendKillErr(e));
        }

        Ok(())
    }

    pub fn gtk_shutdown_signal_checked(
        &self,
        app: &Application,
    ) -> Result<(), ShutdownSettingsErr> {
        let is_active = { *SETTINGS_IS_RUNNING.lock().unwrap() };

        if is_active {
            self.gtk_shutdown_signal(app)
        } else {
            Ok(())
        }
    }

    /// Even when reporting Ok(()), it can seem like it failed if it immediately closes again
    pub fn gtk_open_signal(&self, app: &Application) -> Result<(), OpenSettingsErr> {
        let before = app.configuration.active.clone();

        if let Err(e) = app.external.channels.gpu_sender_request.send(before) {
            return Err(OpenSettingsErr::FailedToUpdateState(e));
        }

        // This is just suggestive. It doesn't hold the lock. It can shutdown before
        // the shutdown call is made or start before the start is called.
        let is_active = { *SETTINGS_IS_RUNNING.lock().unwrap() };

        if is_active {
            match self.gtk_shutdown_signal(app) {
                Err(e) => {
                    return Err(OpenSettingsErr::ThreadPredictedTerminated(e));
                }
                _ => {}
            }
        }

        let res = app.external.channels.start_settings_ui.send(());

        if let Err(e) = res {
            return Err(OpenSettingsErr::FailedToSendStartSignal(e));
        }

        Ok(())
    }
}

impl MirrorRenderer {
    pub fn resize(&self, wgpu: &mut WgpuContainer, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            wgpu.config.width = new_size.width;
            wgpu.config.height = new_size.height;

            wgpu.surface.configure(&wgpu.device, &wgpu.config);

            // This is a hack.
            //
            // Using configure on the GPU pipeline is expensive and causes it to block or queue
            // frames until all of the configuration calls are completed. When resizing the window,
            // calls to configure stack up rapidly which somehow results in what seems like a frame
            // queue of several hundred frames or more...
            //
            // Anyway, blocking the rendering thread for 10 milliseconds seems to prevent rapid calling
            // the configure function during window resizing.
            //
            // Note: 1000 ms / 10 ms = 100 FPS~
            //
            // It shouldn't be very percetible as the blocking time is less than the a normal FPS (ex. 60 FPS),
            // but I think the configure call on the GPU pipeline (surface) is perceptible. Maybe someone running at
            // very high FPS (like 120-300+) would notice it when resizing a window, but they'd probably have to be
            // looking for it.
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn should_render_ui(&self, app: &Application) -> bool {
        if app.user_interaction.mouse_over_screen
            || app.user_interaction.mouse_resize_state != ResizeInteractionsState::None
            || app.app_state.intricate_todo_refactor.in_crop_selection
            || app.app_state.intricate_todo_refactor.crop_button_pressed
        {
            true
        } else {
            false
        }
    }
}

impl Application {
    pub fn get_active_ui_flags(&self) -> Vec<UiFlag> {
        let mut active_ui_flags = vec![];

        {
            if TitleBarDisplay::HiddenTitleBar == self.configuration.active.display_title {
                active_ui_flags.push(UiFlag::DisplayOverlays);
            }

            if self.user_interaction.mouse_over_screen {
                active_ui_flags.push(UiFlag::MouseOverWindow);
            }

            if self.user_interaction.mouse_is_down {
                active_ui_flags.push(UiFlag::MouseDown);
            }

            if self.app_state.intricate_todo_refactor.in_crop_selection
                || self.app_state.intricate_todo_refactor.crop_button_pressed
            {
                active_ui_flags.push(UiFlag::WaitingForCrop);
            }

            if let VideoAspect::MaintainAspectRatio(_, WindowBehaviour::SizeMatchesMirrorAspect) =
                self.configuration.active.aspect_ratio
            {
                active_ui_flags.push(UiFlag::OnlyAngles);
            }

            if self.user_interaction.mouse_resize_state != ResizeInteractionsState::None
                && self.app_state.intricate_todo_refactor.keep_borders
            {
                active_ui_flags.push(UiFlag::KeepBorders);
            }

            if let GreenScreen::Color(_) = self.configuration.active.green_screen {
                active_ui_flags.push(UiFlag::UseGreenScreen);
            }
        }

        active_ui_flags
    }
}

pub const COMPLETE_RESIZE_ON_NEW_SETTINGS_AFTER: i32 = 60;

#[derive(Debug)]
pub enum OpenSettingsErr {
    FailedToUpdateState(SendError<UiState>),
    ThreadPredictedTerminated(ShutdownSettingsErr),
    FailedToSendStartSignal(SendError<()>),
}

#[derive(Debug)]
pub enum ShutdownSettingsErr {
    SendStateErr(SendError<UiState>),
    SendKillErr(SendError<()>),
}
