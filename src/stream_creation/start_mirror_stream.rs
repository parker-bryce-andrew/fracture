use crate::{
    global_application_state::{
        CpuFrame, DmaFrame, FPS_TRACKING, FRAME_TRANSFER, FrameData, FrameLayout, LastReported,
        SAFE_MODE,
    },
    gpu_mirror_display::state::{FpsTracker, FpsTrackerOrigin},
    stream_creation::{
        pipewire_dbus::{
            CursorMode, FreeDesktopPipewireWindowStream, GnomePipewireWindowStream, PipewireStream,
        },
        utility_gnome_video_frame::{
            PredictedWgpuFrameFormat, RealDimensions, WindowDimensionsData, find_real_dimensions,
            guess_best_texture_format,
        },
    },
};
use drm_fourcc::DrmFourcc;
use gnome_window_calls::abstraction::{Window, WindowCache};
use lamco_wgpu::smithay_reexports::{self, Dmabuf};
use mmap::MapOption;
use pipewire::{
    self as pw,
    spa::{buffer::DataType, pod::PropertyFlags},
    stream::StreamRef,
};
use pw::{properties::properties, spa};
use smithay::reexports::ash::vk::Format;
use std::{
    collections::HashMap,
    env,
    ops::Deref,
    os::raw::c_int,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, SystemTime},
};

pub struct StoredBufferData<'a> {
    fd: Option<i64>,
    frame: Arc<LastReported>,
    buffer: pipewire::buffer::Buffer<'a>,
}

pub struct MapStorage<'a, 'b: 'a> {
    pub stream_ref: &'a StreamRef,
    pub active_buffers: Vec<StoredBufferData<'b>>,
}

fn not_queued_buffers<'a, 'b>(s: &mut StreamData<MapStorage<'a, 'b>>) -> isize {
    s.stream.active_buffers.len() as isize
}

fn if_high_dequeued_buffer_count<'a, 'b>(
    total_buffers: Arc<
        Mutex<HashMap<i64, std::sync::Arc<smithay::backend::allocator::dmabuf::Dmabuf>>>,
    >,
    dequeued_buffers: &mut StreamData<MapStorage<'a, 'b>>,
) {
    {
        let buffer_count = || total_buffers.lock().unwrap().len() as isize;

        let mut not_queued = not_queued_buffers(dequeued_buffers);
        let mut queued = buffer_count();

        if not_queued == 0 {
            return;
        }

        if !(not_queued < queued) {
            let text = format!("not queued: {}, max queue size: {}", not_queued, queued);

            println!("{text}");

            println!(
                "The dequeued buffer count is equal to or greater than the buffer count. Attempting to drop buffers."
            );

            let mut attempts = 0;

            {
                'drop_attempt: while !(not_queued < queued) {
                    drop_inactive_buffers(dequeued_buffers);

                    not_queued = not_queued_buffers(dequeued_buffers);
                    queued = buffer_count();

                    if queued <= 2 {
                        println!(
                            "The queued buffer count is less than 2, so we're not going to wait anymore"
                        );
                        break 'drop_attempt;
                    }

                    attempts += 1;

                    if attempts > 100 {
                        println!(
                            "Failed to drop after 100 attempts. Returning control to pipewire and hoping it allocates more buffers."
                        );
                        break 'drop_attempt;
                    }
                }
            }

            println!("There was success in dropping buffers.");
        }
    }
}

fn drop_inactive_buffers(storage: &mut StreamData<MapStorage<'_, '_>>) {
    // Drop buffers to queue them back to pipewire if it is detected no other threads
    // contain the active DmaBuffer.
    {
        let mut new = Vec::new();
        std::mem::swap(&mut storage.stream.active_buffers, &mut new);

        for StoredBufferData { fd, frame, buffer } in new {
            match Arc::try_unwrap(frame) {
                // There are no active copies of the DmaBuffer.
                Ok(_) => {
                    // Just to be explicit.
                    //
                    // This causes the buffer to be queued back to pipewire.
                    std::mem::drop(buffer);
                }
                Err(frame) => {
                    storage.stream.active_buffers.push(StoredBufferData {
                        fd,
                        frame: frame,
                        buffer,
                    });
                }
            }
        }
    }
}

pub struct StreamData<T> {
    pub format: spa::param::video::VideoInfoRaw,
    pub stream: T,
}

fn define_fake_window() -> gnome_window_calls::abstraction::Window {
    Window {
        id: 0,
        cache: WindowCache {
            id: None,
            wm_class: None,
            wm_class_instance: None,
            pid: None,
            maximized: None,
            display: None,
            frame_type: None,
            window_type: None,
            layer: None,
            monitor: None,
            role: None,
            title: None,
            canclose: None,
            canmaximize: None,
            canminimize: None,
            canshade: None,
            moveable: None,
            resizeable: None,
            area: None,
            area_all: None,
            focus: None,
            x: None,
            y: None,
            width: None,
            height: None,
        },
    }
}

pub struct ScanRequest {
    #[allow(unused)]
    request_time: SystemTime,
}

pub fn start_mirroring(
    window: Option<gnome_window_calls::abstraction::Window>,
    dbus_channels: crate::application_channel_creator::DbusSide,
) {
    let have_gnome_window_handle = window.is_some();

    let window: gnome_window_calls::abstraction::Window = if let Some(window) = window {
        window.clone()
    } else {
        define_fake_window()
    };

    let (send_signal_for_change, start_change_scan) = mpsc::channel::<WindowDimensionsData>();
    let (stop_watching_changes, stop_signal) = mpsc::channel::<()>();

    let mut copy = window.clone();

    let window_monitoring_handle = std::thread::spawn(move || {
        if have_gnome_window_handle {
            let mut last_w = copy.cache.width.unwrap_or(1920);
            let mut last_h = copy.cache.height.unwrap_or(1080);
            let mut last_is_maximized = copy.cache.maximized.unwrap_or(0);

            'change_watch: loop {
                let _ = copy.refresh();

                if (last_w, last_h, last_is_maximized)
                    != (
                        copy.cache.width.unwrap_or(1920),
                        copy.cache.height.unwrap_or(1080),
                        copy.cache.maximized.unwrap_or(0),
                    )
                {
                    last_w = copy.cache.width.unwrap_or(1920).clone();
                    last_h = copy.cache.height.unwrap_or(1080).clone();
                    last_is_maximized = copy.cache.maximized.unwrap_or(0).clone();

                    let temp = WindowDimensionsData {
                        x: copy.cache.x.unwrap_or(0) as i64,
                        y: copy.cache.y.unwrap_or(0) as i64,
                        width: last_w as i64,
                        height: last_h as i64,
                        maximized: copy.cache.maximized,
                    };

                    if let Err(e) = send_signal_for_change.send(temp) {
                        println!(
                            "This channel should stay open forever. This should only happen if a shutdown happened: {e:?}"
                        );
                    }
                }

                match stop_signal.recv_timeout(Duration::from_secs(1)) {
                    Ok(_) => {
                        break 'change_watch;
                    }
                    Err(_) => {}
                }
            }
        } else {
            'change_watch: loop {
                // It was originally written for Gnome, then changed to handle any Linux DE
                // that supports XDG Desktop Portals.
                let fake_data = WindowDimensionsData {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                    maximized: Some(0),
                };

                if let Err(e) = send_signal_for_change.send(fake_data) {
                    println!(
                        "This channel should stay open forever. This should only happen if a shutdown happened: {e:?}"
                    );
                }

                match stop_signal.recv_timeout(Duration::from_secs(5)) {
                    Ok(_) => {
                        break 'change_watch;
                    }
                    Err(_) => {}
                }
            }
        }
    });

    #[derive(Debug, Clone)]
    enum FrameScanner {
        RunScan,
        StopScanning,
    }

    let (run_frame_scan, received_signal_to_run) = std::sync::mpsc::channel::<FrameScanner>();
    let (cs2, frame_scan_results) = std::sync::mpsc::channel::<RealDimensions>();
    let gpu_scan_requester = dbus_channels.gpu_frame_scan_requested.clone();
    let is_gnome = have_gnome_window_handle;

    let scan_frame_loop = std::thread::spawn(move || {
        'scanning: loop {
            if let Ok(mut op) = received_signal_to_run.recv() {
                while let Ok(later_op) = received_signal_to_run.try_recv() {
                    op = later_op;
                }

                let mut request_time = SystemTime::now();
                let _ = gpu_scan_requester.send(ScanRequest {
                    request_time: request_time,
                });

                let mut previous_scan_result = None;

                let mut match_count = 0;

                let target = if is_gnome { 30 } else { 1 };

                match op {
                    FrameScanner::RunScan => 'continue_until_success: loop {
                        while let Ok(later_op) = received_signal_to_run.try_recv() {
                            op = later_op;
                            match_count = 0;

                            if let FrameScanner::StopScanning = op {
                                break 'scanning;
                            }
                        }

                        let frame_data = {
                            let temp: &Option<Arc<LastReported>> = &*FRAME_TRANSFER.lock().unwrap();

                            if let Some(frame) = temp {
                                Some(Arc::clone(frame))
                            } else {
                                None
                            }
                        };

                        if let Some(data) = frame_data {
                            let temp: &FrameData = &data.frame_data;

                            let temp: Option<&CpuFrame> = match temp {
                                FrameData::CpuData(cpu_frame) => Some(cpu_frame),
                                FrameData::DmaBuffers(dma_frame) => {
                                    if let Some(v) = &dma_frame.saved_cpu_frame {
                                        let temp: &CpuFrame = &v;

                                        Some(temp)
                                    } else {
                                        None
                                    }
                                }
                            };

                            let temp = match temp {
                                Some(cpu_frame) => {
                                    if cpu_frame.scan_time > request_time {
                                        Some(cpu_frame)
                                    } else {
                                        None
                                    }
                                }
                                None => None,
                            };

                            match temp {
                                Some(frame) => {
                                    let frame: &CpuFrame = frame;

                                    let FrameLayout {
                                        width,
                                        height,
                                        bytes_per_pixel: _,
                                    } = frame.layout;
                                    let data: &Vec<u8> = &frame.frame_data;

                                    // println!("scan started: {:?}", SystemTime::now());

                                    let real_dimensions =
                                        find_real_dimensions(&data, &(width as i32, height as i32));

                                    if let Some(value) = previous_scan_result {
                                        let _ = cs2.send(real_dimensions.clone());

                                        if value == real_dimensions {
                                            match_count += 1;
                                        } else {
                                            match_count = 0;
                                        }

                                        if match_count >= target {
                                            break 'continue_until_success;
                                        }
                                    }

                                    // The first (or subsequent) scan results did not
                                    // match the current scan results, so we check
                                    // a frame again because it's guessing that the window
                                    // size is still changing.
                                    previous_scan_result = Some(real_dimensions);
                                    request_time = SystemTime::now();

                                    std::thread::sleep(Duration::from_millis(10));

                                    let _ = gpu_scan_requester.send(ScanRequest {
                                        request_time: request_time,
                                    });
                                }
                                None => {
                                    continue;
                                }
                            }
                        }

                        std::thread::sleep(Duration::from_millis(10));
                    },
                    FrameScanner::StopScanning => {
                        break 'scanning;
                    }
                }
            } else {
                break 'scanning;
            }
        }
    });

    let pipewire_window_stream: Result<Box<dyn PipewireStream>, ()> = {
        let cursor = CursorMode::Hidden;

        if have_gnome_window_handle {
            Ok(Box::new(GnomePipewireWindowStream::create_stream(
                &window, cursor,
            )))
        } else {
            let result = FreeDesktopPipewireWindowStream::create_stream(&window, cursor);

            if result.is_err() {
                Err(())
            } else {
                Ok(Box::new(result.unwrap()))
            }
        }
    };

    let is_ok = pipewire_window_stream.is_ok();

    dbus_channels
        .stream_start_check_mirror_gpu
        .send(is_ok)
        .unwrap();
    dbus_channels
        .stream_start_check_settings_ui
        .send(is_ok)
        .unwrap();

    if !is_ok {
        return;
    }

    let pipewire_window_stream = pipewire_window_stream.unwrap();

    pw::init();

    let mainloop = pw::main_loop::MainLoop::new(None).unwrap();

    let mainloop_copy = mainloop.clone();

    let _receiver_handle = dbus_channels.terminate_signal_receiver.attach(
        &mainloop.loop_(),
        move |_terminate_received: ()| {
            println!("Shutting down the stream.");

            mainloop_copy.quit();
        },
    );

    let context = pw::context::Context::new(&mainloop).unwrap();
    let core = context.connect(None).unwrap();

    let stream = pw::stream::Stream::new(
        &core,
        "video-test",
        properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .unwrap();

    let for_store: &StreamRef = stream.deref();

    let map = Vec::new();

    let testing = MapStorage {
        stream_ref: for_store,
        active_buffers: map,
    };

    let meta = StreamData {
        format: Default::default(),
        stream: testing,
    };

    let mut last_known_window_dimensions = WindowDimensionsData {
        x: window.cache.x.unwrap_or(0) as i64,
        y: window.cache.y.unwrap_or(0) as i64,
        width: window.cache.width.unwrap_or(1920) as i64,
        height: window.cache.height.unwrap_or(1080) as i64,
        maximized: window.cache.maximized,
    };

    let mut last_known_offsets = (0, 0);

    // This should calculate the offsets on the first frame received.
    let mut offset_countdown: Option<i32> = Some(-1);
    let mut first_offset_call = true;

    let frame_scan_stopper = run_frame_scan.clone();
    let _ = frame_scan_stopper.send(FrameScanner::RunScan);

    let buffers: Arc<
        Mutex<HashMap<i64, std::sync::Arc<smithay::backend::allocator::dmabuf::Dmabuf>>>,
    > = Arc::new(Mutex::new(HashMap::new()));

    let remove_buffer_copy = buffers.clone();

    let mut last_format_dimensions = None;

    let run_scan_meta = run_frame_scan.clone();

    let without_dma_modifiers = pipewire::spa::pod::Object {
        type_: (pw::spa::utils::SpaTypes::ObjectParamFormat).as_raw(),
        id: (pw::spa::param::ParamType::EnumFormat).as_raw(),
        properties: [
            (pipewire::spa::pod::Property {
                key: (pw::spa::param::format::FormatProperties::MediaType).as_raw(),
                flags: pipewire::spa::pod::PropertyFlags::empty(),
                value: (pipewire::spa::pod::Value::Id(pipewire::spa::utils::Id(
                    (pw::spa::param::format::MediaType::Video).as_raw(),
                ))),
            }),
            (pipewire::spa::pod::Property {
                key: (pw::spa::param::format::FormatProperties::MediaSubtype).as_raw(),
                flags: pipewire::spa::pod::PropertyFlags::empty(),
                value: (pipewire::spa::pod::Value::Id(pipewire::spa::utils::Id(
                    (pw::spa::param::format::MediaSubtype::Raw).as_raw(),
                ))),
            }),
            (pipewire::spa::pod::Property {
                key: (pw::spa::param::format::FormatProperties::VideoFormat).as_raw(),
                flags: pipewire::spa::pod::PropertyFlags::empty(),
                value: (pipewire::spa::pod::Value::Choice(pipewire::spa::pod::ChoiceValue::Id(
                    pipewire::spa::utils::Choice::<pipewire::spa::utils::Id>(
                        pipewire::spa::utils::ChoiceFlags::empty(),
                        pipewire::spa::utils::ChoiceEnum::Enum {
                            default: pipewire::spa::utils::Id(
                                (pw::spa::param::video::VideoFormat::BGRA).as_raw(),
                            ),
                            alternatives: [
                                pipewire::spa::utils::Id(
                                    (pw::spa::param::video::VideoFormat::BGRA).as_raw(),
                                ),
                                pipewire::spa::utils::Id(
                                    (pw::spa::param::video::VideoFormat::RGBA).as_raw(),
                                ),
                                pipewire::spa::utils::Id(
                                    (pw::spa::param::video::VideoFormat::RGB).as_raw(),
                                ),
                                pipewire::spa::utils::Id(
                                    (pw::spa::param::video::VideoFormat::RGBA).as_raw(),
                                ),
                                pipewire::spa::utils::Id(
                                    (pw::spa::param::video::VideoFormat::RGBx).as_raw(),
                                ),
                                pipewire::spa::utils::Id(
                                    (pw::spa::param::video::VideoFormat::BGRx).as_raw(),
                                ),
                            ]
                            .to_vec(),
                        },
                    ),
                ))),
            }),
        ]
        .to_vec(),
    };

    let should_track_fps = env::var(FPS_TRACKING).is_ok();
    let state_change_pod_values = without_dma_modifiers.clone();
    let mut fps_tracker = FpsTracker::new(
        Some(FpsTrackerOrigin::Pipewire),
        Some(vec![
            Duration::from_secs(1),
            Duration::from_secs(15),
            Duration::from_secs(60),
            Duration::from_secs(60 * 5),
            Duration::from_secs(60 * 15),
            Duration::from_secs(60 * 60),
        ]),
    );

    let _listener = stream
        .add_local_listener_with_user_data(meta)
        .add_buffer(|_, _, pw| unsafe {
            let temp = &*pw;
            let temp = &*temp.buffer;
            let temp = &*temp.datas;
            let buff_fd = temp.fd;

            println!("buffer with fd '{}' added", buff_fd);
        })
        .remove_buffer(move |_, storage, pw| {
            unsafe {
                let temp = &*pw;
                let temp = &*temp.buffer;
                let temp = &*temp.datas;
                let buff_fd = temp.fd;

                println!("buffer with fd '{}' removed", buff_fd);

                // The shutdown panics if the buffers are not removed from memory
                // when the shutdown is requested. This still doesn't seem correct
                // because this can leave an active DmaBuffer with an active file
                // descriptor open.
                //
                // It does seem to fix the panic... but maybe it's UB? I don't know.
                {
                    if let Some((idx, _)) =
                        storage
                            .stream
                            .active_buffers
                            .iter()
                            .enumerate()
                            .find(|(_, buffer)| {
                                if let Some(id) = buffer.fd {
                                    id == buff_fd
                                } else {
                                    false
                                }
                            })
                    {
                        storage.stream.active_buffers.remove(idx);
                    }
                }

                let _ = remove_buffer_copy.lock().unwrap().remove(&buff_fd);
            }
        })
        .state_changed(move |stream_ref, _, old, new| {
            match stream_ref.state() {
                v @ pipewire::stream::StreamState::Error(_) => {
                    println!("{:#?}", v);

                    println!("Attempting to downgrade DmaBuffers to CpuBuffers");

                    // For now I'm just going to assume that errors are related
                    // to not finding a way to negotiate the stream. The only
                    // option available at the moment is to downgrade the stream
                    // from DmaBuffers to CpuBuffers.
                    //
                    // I don't think errors should ever close the application. The user
                    // might be running shaders that they want have continue running after
                    // the stream ends.

                    let temp = state_change_pod_values.clone();

                    let temp: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
                        std::io::Cursor::new(Vec::new()),
                        &pw::spa::pod::Value::Object(temp),
                    )
                    .unwrap()
                    .0
                    .into_inner();

                    let mut parameters = [spa::pod::Pod::from_bytes(&temp).unwrap()];

                    let _ = stream_ref.update_params(&mut parameters);
                }
                _ => {}
            }

            println!("State changed: {:?} -> {:?}", old, new);
        })
        .param_changed(move |_, storage, id, param| {
            let Some(param) = param else {
                return;
            };

            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }

            let (media_type, media_subtype) =
                match pw::spa::param::format_utils::parse_format(param) {
                    Ok(v) => v,
                    Err(_) => return,
                };

            if media_type != pw::spa::param::format::MediaType::Video
                || media_subtype != pw::spa::param::format::MediaSubtype::Raw
            {
                return;
            }

            storage
                .format
                .parse(param)
                .expect("Failed to parse param changed to VideoInfoRaw");

            let temp = PredictedWgpuFrameFormat {
                format: guess_best_texture_format(storage.format.format()),
                width: storage.format.size().width,
                height: storage.format.size().height,
            };

            let _ = dbus_channels.predicted_frame_fmt_sender.send(temp);

            println!(
                "Video Format: {} ({:?})",
                storage.format.format().as_raw(),
                storage.format.format()
            );

            println!(
                "Size: {}x{}",
                storage.format.size().width,
                storage.format.size().height
            );

            let new_wh = (storage.format.size().width, storage.format.size().height);

            if let Some(old_wh) = last_format_dimensions {
                if new_wh != old_wh {
                    println!("requested scan");
                    let _ = run_scan_meta.send(FrameScanner::RunScan);
                }
            }
        })
        .process(move |_, storage| {
            drop_inactive_buffers(storage);

            let mut buffer_fd_id_opt = None;

            let new_wh = Some((storage.format.size().width, storage.format.size().height));
            let old_wh = last_format_dimensions;

            if new_wh != old_wh {
                println!("requested scan");
                let _ = run_frame_scan.send(FrameScanner::RunScan);

                last_known_window_dimensions.width = new_wh.as_ref().unwrap().0 as i64;
                last_known_window_dimensions.height = new_wh.as_ref().unwrap().1 as i64;
                last_format_dimensions = new_wh;
            }

            let mut buffer = None;

            if let Some(buf) = storage.stream.stream_ref.dequeue_buffer() {
                buffer = Some(buf);
            }

            if buffer.is_none() {
                println!("out of buffers");
                return;
            }

            let mut buffer_for_storage = buffer.unwrap();

            {
                if should_track_fps {
                    fps_tracker.increment();

                    if let Ok(_) = dbus_channels.fps_request.try_recv() {
                        println!(
                            "------------------------------------------------------------------"
                        );
                        println!("{:?}", fps_tracker.report());
                    }
                }

                let buffer_data: &mut [spa::buffer::Data] = buffer_for_storage.datas_mut();

                if buffer_data.len() == 0 || buffer_data.is_empty() {
                    return;
                }

                let frame_data = match buffer_data[0].type_() {
                    DataType::DmaBuf => {
                        let buffer_fd_id = buffer_data[0].as_raw().fd;

                        if buffer_fd_id == -1 {
                            return;
                        }

                        buffer_fd_id_opt = Some(buffer_fd_id);

                        let result = {
                            let buffers = &mut *buffers.lock().unwrap();

                            if buffers.contains_key(&buffer_fd_id) {
                                buffers.get(&buffer_fd_id).unwrap().clone()
                            } else {
                                let stride = buffer_data[0].chunk().stride();
                                let size = buffer_data[0].chunk().size() as i32;

                                let fmt = match storage.format.format().0 {
                                    // rgbx
                                    7 => DrmFourcc::Rgba8888,
                                    // bgrx
                                    8 => DrmFourcc::Bgra8888,
                                    // rgba
                                    11 => DrmFourcc::Rgba8888,
                                    // bgra
                                    12 => DrmFourcc::Bgra8888,
                                    // rgb
                                    15 => DrmFourcc::Rgba8888,
                                    // unknown
                                    0.. => DrmFourcc::Bgra8888,
                                };

                                let mut builder = smithay_reexports::Dmabuf::builder(
                                    ((stride / 4) as i32, ((size / 4) / (stride / 4)) as i32),
                                    // todo: fix format
                                    fmt,
                                    drm_fourcc::DrmModifier::from(storage.format.modifier()),
                                    smithay::backend::allocator::dmabuf::DmabufFlags::empty(),
                                );

                                let mut stride = 0;
                                let mut size = 0;

                                let buffer_data: Vec<_> = buffer_data
                                    .iter()
                                    .enumerate()
                                    .map(|(idx, plane)| {
                                        let fd: c_int = plane.as_raw().fd as c_int;
                                        let fd: std::os::fd::RawFd = fd;
                                        let borrowed =
                                            unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };
                                        let fd: Result<_, _> = borrowed.try_clone_to_owned();

                                        if fd.is_err() {
                                            return None;
                                        }

                                        let fd = fd.unwrap();

                                        stride = plane.chunk().stride();
                                        size = plane.chunk().size();

                                        Some((
                                            fd,
                                            idx as u32,
                                            plane.chunk().offset() as u32,
                                            plane.chunk().stride() as u32,
                                        ))
                                    })
                                    .collect();

                                if buffer_data.iter().any(|v| v.is_none()) {
                                    println!("error importing dma buffer");

                                    return;
                                }

                                buffer_data.into_iter().map(|v| v.unwrap()).for_each(
                                    |(fd, idx, offset, stride)| {
                                        builder.add_plane(fd, idx, offset, stride);
                                    },
                                );

                                let result: Option<smithay_reexports::Dmabuf> = builder.build();

                                if result.is_none() {
                                    println!("failed to build dma buffer");

                                    return;
                                }

                                let result = result.unwrap();

                                let temp = Arc::new(result);

                                buffers.insert(buffer_fd_id, temp.clone());

                                temp
                            }
                        };

                        let dma: &Dmabuf = &result;
                        let dma: Dmabuf = dma.clone();

                        let dma = DmaFrame {
                            frame_data: dma,

                            // This is set when we lock to update globally
                            // to avoid accidentlaly overwriting
                            // data sent from the rendering thread
                            saved_cpu_frame: None,
                        };

                        FrameData::DmaBuffers(dma)
                    }

                    // Everything else is assumed to be CPU data
                    _ => {
                        let temp = buffer_data;
                        let data = temp[0].data();

                        let data = {
                            if let Some(data) = data {
                                data.to_vec()
                            } else {
                                let fd = temp[0].as_raw().fd;

                                if fd == -1 {
                                    return;
                                }

                                let chunk = temp[0].as_raw().chunk;
                                let chunk = unsafe { &*chunk };

                                debug_assert_eq!(
                                    0, chunk.offset,
                                    "Non-zero chunk offsets are not supported yet"
                                );

                                let data = mmap::MemoryMap::new(
                                    chunk.size as usize,
                                    &[
                                        MapOption::MapReadable,
                                        MapOption::MapFd(fd as i32),
                                        MapOption::MapOffset(temp[0].as_raw().mapoffset as usize),
                                    ],
                                );

                                if data.is_err() {
                                    if let Err(err) = data {
                                        println!("{}: {:#?}", "buffer map error", err);
                                    }

                                    return;
                                }

                                let data_map = data.unwrap();
                                let data_array_len = data_map.len();
                                let data_ptr = data_map.data() as *const u8;

                                let data: &[u8] =
                                    unsafe { std::slice::from_raw_parts(data_ptr, data_array_len) };

                                assert_eq!(data.len(), chunk.size as usize);

                                data.to_vec()
                            }
                        };

                        let temp = CpuFrame {
                            frame_data: data,
                            layout: FrameLayout {
                                width: storage.format.size().width as u32,
                                height: storage.format.size().height as u32,
                                bytes_per_pixel: 4,
                            },
                            scan_time: SystemTime::now(),
                        };

                        FrameData::CpuData(temp)
                    }
                };

                while let Ok(val) = start_change_scan.try_recv() {
                    // The other environments scan frames every 5 seconds for now.
                    if have_gnome_window_handle {
                        last_known_window_dimensions = val;

                        // If the offsets are not reset the requested dimensions can surpass the buffer size.
                        //
                        // This happens because the buffer size can be something like exactly 2160x3840 while
                        // offsets of on the x and y are like (15, 12). This results in requested buffer data
                        // from (2160+12, 3840+15) which is larger than the buffer.
                        last_known_offsets = (0, 0);
                    }

                    // This is no longer used and is set to -1 to immediately start scans.
                    {
                        if !first_offset_call {
                            offset_countdown = Some(-1);
                        }
                    }

                    first_offset_call = false;
                }

                // This is no longer used
                {
                    if let Some(count) = &mut offset_countdown {
                        *count -= 1;
                    }
                }

                if let Ok(val) = frame_scan_results.try_recv() {
                    let RealDimensions {
                        off_x,
                        off_y,
                        width,
                        height,
                    } = val;

                    last_known_offsets = (off_x, off_y);

                    last_known_window_dimensions.width = width as i64;
                    last_known_window_dimensions.height = height as i64;
                }

                if let Some(count) = &offset_countdown {
                    if *count < 0 {
                        offset_countdown = None;

                        let _ = run_frame_scan.send(FrameScanner::RunScan);

                        /*
                           There are some slight issues with Gnome's video recorder

                           For fullscreen, non-maximized, non-fullscreen. The window shape is 2160x3840, but pipewire always maxes out at that size. When Gnome
                           adds it's offsets, it destroys the video frame. This can't be fixed here, and is an upstream problem. This impl goes as
                           far as possible by detecting incorrectly reported window sizes.

                        */
                    }
                }

                let temp = LastReported {
                    window_dimensions: (
                        last_known_window_dimensions.width as u32,
                        last_known_window_dimensions.height as u32,
                    ),
                    frame_data: Arc::new(frame_data),
                    last_known_offsets,
                };

                let mut frame = temp;

                {
                    let mut lock = FRAME_TRANSFER.lock().unwrap();

                    let last_saved: Option<Arc<CpuFrame>> = {
                        match &*lock {
                            Some(v) => match &*v.frame_data {
                                FrameData::CpuData(cpu_frame) => Some(Arc::new(cpu_frame.clone())),
                                FrameData::DmaBuffers(dma_frame) => {
                                    dma_frame.saved_cpu_frame.clone()
                                }
                            },
                            None => None,
                        }
                    };

                    match { &*frame.frame_data } {
                        FrameData::DmaBuffers(dma_frame) => {
                            let temp: &DmaFrame = dma_frame;
                            let mut temp: DmaFrame = temp.clone();
                            temp.saved_cpu_frame = last_saved;

                            frame.frame_data = Arc::new(FrameData::DmaBuffers(temp));
                        }
                        _ => {}
                    }

                    let frame = Arc::new(frame);

                    let temp = StoredBufferData {
                        fd: buffer_fd_id_opt,
                        frame: Arc::clone(&frame),
                        buffer: buffer_for_storage,
                    };

                    // don't watch the buffers if they're not DmaBuffers.
                    if buffer_fd_id_opt.is_some() {
                        storage.stream.active_buffers.push(temp);
                    }

                    *lock = Some(frame);
                }

                let _ = dbus_channels.new_frame_notifier.send(());
            }

            if_high_dequeued_buffer_count(buffers.clone(), storage);
        })
        .register()
        .unwrap();

    println!("blocked here");

    let mut report = Some(dbus_channels.webgpu_report_receiver.recv().unwrap());

    if !report.as_ref().unwrap().using_bridge {
        report = None;
    }

    println!("report recv");

    let mods: Option<_> = {
        if let Some(report) = report {
            if let Some(mut v) = report.formats {
                let bgra = v
                    .iter()
                    .find(|v| v.vk_format == Format::B8G8R8A8_UNORM)
                    .map(|v| v.clone())
                    .unwrap();
                let rgba = v
                    .iter()
                    .find(|v| v.vk_format == Format::R8G8B8A8_UNORM)
                    .map(|v| v.clone())
                    .unwrap();

                v.insert(0, rgba.clone());
                v.insert(0, bgra.clone());

                let mods: Vec<_> = v
                    .iter()
                    .map(|v| {
                        v.modifier_props
                            .iter()
                            .filter(|prop| prop.plane_count == 1)
                            .map(|m| m.modifier as i64)
                    })
                    .flatten()
                    .collect();

                let temp = pipewire::spa::pod::Property {
                    key: (pw::spa::param::format::FormatProperties::VideoModifier).as_raw(),
                    flags: PropertyFlags::empty(),
                    value: (pipewire::spa::pod::Value::Choice(
                        pipewire::spa::pod::ChoiceValue::Long(pipewire::spa::utils::Choice::<i64>(
                            pipewire::spa::utils::ChoiceFlags::empty(),
                            pipewire::spa::utils::ChoiceEnum::Enum {
                                default: mods[0],
                                alternatives: mods,
                            },
                        )),
                    )),
                };

                Some(temp)
            } else {
                None
            }
        } else {
            None
        }
    };

    let mut obj = without_dma_modifiers.clone();

    if let Some(mods) = mods {
        println!("DmaBuffer modifiers are specified.");

        if let Err(_) = std::env::var(SAFE_MODE) {
            println!("Attempting to use DmaBuffer modifiers with pipewire.");

            obj.properties.push(mods);
        } else {
            println!("Skipping DmaBuffer modifiers with pipewire because SAFE_MODE is active.");
        }
    } else {
        println!("DmaBuffer modifiers are missing.");
    }

    println!("Created stream {:#?}", stream);

    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .unwrap()
    .0
    .into_inner();

    let mut parameters = [spa::pod::Pod::from_bytes(&values).unwrap()];

    stream
        .connect(
            spa::utils::Direction::Input,
            Some(pipewire_window_stream.id()),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut parameters,
        )
        .unwrap();

    println!("Connected stream");

    mainloop.run();

    // Shutdown the window monitoring spawned on this thread.
    let _ = stop_watching_changes.send(());
    let _ = frame_scan_stopper.send(FrameScanner::StopScanning);
    window_monitoring_handle.join().unwrap();
    scan_frame_loop.join().unwrap();
}
