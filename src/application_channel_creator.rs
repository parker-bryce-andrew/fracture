use std::sync::Arc;

use lamco_wgpu::smithay_reexports::{self};

use crate::{
    gpu_mirror_display::event_loop::WebGpuReport,
    stream_creation::{
        start_mirror_stream::ScanRequest, utility_gnome_video_frame::PredictedWgpuFrameFormat,
    },
    ui_state::UiState,
};

pub struct ApplicationChannelsCreator;

pub struct GpuChannelSide {
    pub start_settings_ui: std::sync::mpsc::Sender<()>,
    pub new_settings_receiver: std::sync::mpsc::Receiver<UiState>,
    pub gpu_sender_request: std::sync::mpsc::Sender<UiState>,
    pub predicted_frame_fmt_receiver: std::sync::mpsc::Receiver<PredictedWgpuFrameFormat>,
    pub terminate_pipewire_stream: pipewire::channel::Sender<()>,
    pub terminate_settings_ui: std::sync::mpsc::Sender<()>,
    pub webgpu_drm_report: std::sync::mpsc::Sender<WebGpuReport>,
    pub dmabuf_rec: std::sync::mpsc::Receiver<Arc<smithay_reexports::Dmabuf>>,
    pub gpu_frame_scan_requested: std::sync::mpsc::Receiver<ScanRequest>,
    pub ui_shutdown_conf: std::sync::mpsc::Receiver<()>,
    pub dbus_shutdown_conf: std::sync::mpsc::Receiver<()>,
    pub stream_start_check_mirror_gpu: std::sync::mpsc::Receiver<bool>,
    pub kill_gtk: std::sync::mpsc::Sender<()>,
    pub request_pipewire_fps: std::sync::mpsc::Sender<()>,
}

pub struct UiChannelSide {
    pub start_signal_receiver: std::sync::mpsc::Receiver<()>,
    pub updated_state_sender: std::sync::mpsc::Sender<UiState>,
    pub gpu_receiver_request: std::sync::mpsc::Receiver<UiState>,
    pub stop_settings_ui: std::sync::mpsc::Receiver<()>,
    pub shutdown_confirmed: std::sync::mpsc::Sender<()>,
    pub stream_start_check_settings_ui: std::sync::mpsc::Receiver<bool>,
    pub kill_with_confirm_recv: std::sync::mpsc::Receiver<()>,
}

pub struct DbusSide {
    pub predicted_frame_fmt_sender: std::sync::mpsc::Sender<PredictedWgpuFrameFormat>,
    pub terminate_signal_receiver: pipewire::channel::Receiver<()>,
    pub webgpu_report_receiver: std::sync::mpsc::Receiver<WebGpuReport>,
    pub dmabuf_send: std::sync::mpsc::Sender<Arc<smithay_reexports::Dmabuf>>,
    pub gpu_frame_scan_requested: std::sync::mpsc::Sender<ScanRequest>,
    pub shutdown_confirmed: std::sync::mpsc::Sender<()>,
    pub stream_start_check_mirror_gpu: std::sync::mpsc::Sender<bool>,
    pub stream_start_check_settings_ui: std::sync::mpsc::Sender<bool>,
    pub fps_request: std::sync::mpsc::Receiver<()>,
}

impl ApplicationChannelsCreator {
    pub fn channels() -> (GpuChannelSide, UiChannelSide, DbusSide) {
        let (s1, r1) = std::sync::mpsc::channel::<_>();
        let (s2, r2) = std::sync::mpsc::channel::<_>();
        let (s3, r3) = std::sync::mpsc::channel::<_>();
        let (s4, r4) = std::sync::mpsc::channel::<_>();
        let (s5, r5) = pipewire::channel::channel::<_>();
        let (s6, r6) = std::sync::mpsc::channel::<_>();
        let (s7, r7) = std::sync::mpsc::channel::<_>();
        let (s8, r8) = std::sync::mpsc::channel::<_>();
        let (s9, r9) = std::sync::mpsc::channel::<_>();
        let (s10, r10) = std::sync::mpsc::channel::<_>();
        let (s11, r11) = std::sync::mpsc::channel::<_>();
        let (s12, r12) = std::sync::mpsc::channel::<_>();
        let (s13, r13) = std::sync::mpsc::channel::<_>();
        let (s14, r14) = std::sync::mpsc::channel::<_>();
        let (s15, r15) = std::sync::mpsc::channel::<_>();

        (
            GpuChannelSide {
                start_settings_ui: s1,
                new_settings_receiver: r2,
                gpu_sender_request: s3,
                predicted_frame_fmt_receiver: r4,
                terminate_pipewire_stream: s5,
                terminate_settings_ui: s6,
                webgpu_drm_report: s7,
                dmabuf_rec: r8,
                gpu_frame_scan_requested: r9,
                ui_shutdown_conf: r10,
                dbus_shutdown_conf: r11,
                stream_start_check_mirror_gpu: r12,
                kill_gtk: s14,
                request_pipewire_fps: s15,
            },
            UiChannelSide {
                start_signal_receiver: r1,
                updated_state_sender: s2,
                gpu_receiver_request: r3,
                stop_settings_ui: r6,
                shutdown_confirmed: s10,
                stream_start_check_settings_ui: r13,
                kill_with_confirm_recv: r14,
            },
            DbusSide {
                predicted_frame_fmt_sender: s4,
                terminate_signal_receiver: r5,
                webgpu_report_receiver: r7,
                dmabuf_send: s8,
                gpu_frame_scan_requested: s9,
                shutdown_confirmed: s11,
                stream_start_check_mirror_gpu: s12,
                stream_start_check_settings_ui: s13,
                fps_request: r15,
            },
        )
    }
}
