use crate::{
    global_application_state::{
        CpuFrame, DmaFrame, FRAME_TRANSFER, FrameData, FrameLayout, LastReported, SAFE_MODE,
    },
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
};
use pw::{properties::properties, spa};
use smithay::reexports::ash::vk::Format;
use std::{
    collections::HashMap,
    os::{
        fd::{FromRawFd, OwnedFd},
        raw::c_int,
    },
    sync::{Arc, Mutex, mpsc},
    time::{Duration, SystemTime},
};

pub struct StreamData {
    pub format: spa::param::video::VideoInfoRaw,
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

    let meta = StreamData {
        format: Default::default(),
    };

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

    let _listener = stream
        .add_local_listener_with_user_data(meta)
        .add_buffer(|_, _, pw| unsafe {
            let temp = &*pw;
            let temp = &*temp.buffer;
            let temp = &*temp.datas;
            let buff_fd = temp.fd;

            println!("buffer with fd '{}' added", buff_fd);
        })
        .remove_buffer(move |_, _, pw| unsafe {
            let temp = &*pw;
            let temp = &*temp.buffer;
            let temp = &*temp.datas;
            let buff_fd = temp.fd;

            println!("buffer with fd '{}' removed", buff_fd);

            let _ = remove_buffer_copy.lock().unwrap().remove(&buff_fd);
        })
        .state_changed(|_, _, old, new| {
            println!("State changed: {:?} -> {:?}", old, new);
        })
        .param_changed(move |_, meta, id, param| {
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

            meta.format
                .parse(param)
                .expect("Failed to parse param changed to VideoInfoRaw");

            let temp = PredictedWgpuFrameFormat {
                format: guess_best_texture_format(meta.format.format()),
                width: meta.format.size().width,
                height: meta.format.size().height,
            };

            let _ = dbus_channels.predicted_frame_fmt_sender.send(temp);

            println!(
                "Video Format: {} ({:?})",
                meta.format.format().as_raw(),
                meta.format.format()
            );

            println!(
                "Size: {}x{}",
                meta.format.size().width,
                meta.format.size().height
            );

            let new_wh = (meta.format.size().width, meta.format.size().height);

            if let Some(old_wh) = last_format_dimensions {
                if new_wh != old_wh {
                    println!("requested scan");
                    let _ = run_scan_meta.send(FrameScanner::RunScan);
                }
            }
        })
        .process(move |stream, meta| {
            let new_wh = Some((meta.format.size().width, meta.format.size().height));
            let old_wh = last_format_dimensions;

            if new_wh != old_wh {
                println!("requested scan");
                let _ = run_frame_scan.send(FrameScanner::RunScan);

                last_known_window_dimensions.width = new_wh.as_ref().unwrap().0 as i64;
                last_known_window_dimensions.height = new_wh.as_ref().unwrap().1 as i64;
                last_format_dimensions = new_wh;
            }

            match stream.dequeue_buffer() {
                None => println!("out of buffers"),
                Some(mut buffer) => {
                    let buffer_data: &mut [spa::buffer::Data] = buffer.datas_mut();

                    if buffer_data.len() == 0 || buffer_data.is_empty() {
                        return;
                    }

                    let frame_data = match buffer_data[0].type_() {
                        DataType::DmaBuf => {
                            let buffer_fd_id = buffer_data[0].as_raw().fd;

                            if buffer_fd_id == -1 {
                                return;
                            }

                            let result = {
                                let buffers = &mut *buffers.lock().unwrap();

                                if buffers.contains_key(&buffer_fd_id) {
                                    buffers.get(&buffer_fd_id).unwrap().clone()
                                } else {
                                    let stride = buffer_data[0].chunk().stride();
                                    let size = buffer_data[0].chunk().size() as i32;

                                    let fmt = match meta.format.format().0 {
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
                                        drm_fourcc::DrmModifier::from(meta.format.modifier()),
                                        smithay::backend::allocator::dmabuf::DmabufFlags::empty(),
                                    );

                                    let mut stride = 0;
                                    let mut size = 0;

                                    buffer_data.iter().enumerate().for_each(|(idx, plane)| {
                                        let fd: c_int = plane.as_raw().fd as c_int;
                                        let fd: std::os::fd::RawFd = fd;
                                        let fd: std::os::fd::OwnedFd =
                                            unsafe { OwnedFd::from_raw_fd(fd) };

                                        stride = plane.chunk().stride();
                                        size = plane.chunk().size();

                                        builder.add_plane(
                                            fd,
                                            idx as u32,
                                            plane.chunk().offset() as u32,
                                            plane.chunk().stride() as u32,
                                        );
                                    });

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
                                            MapOption::MapOffset(
                                                temp[0].as_raw().mapoffset as usize,
                                            ),
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

                                    let data: &[u8] = unsafe {
                                        std::slice::from_raw_parts(data_ptr, data_array_len)
                                    };

                                    assert_eq!(data.len(), chunk.size as usize);

                                    data.to_vec()
                                }
                            };

                            let temp = CpuFrame {
                                frame_data: data,
                                layout: FrameLayout {
                                    width: meta.format.size().width as u32,
                                    height: meta.format.size().height as u32,
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
                                    FrameData::CpuData(cpu_frame) => {
                                        Some(Arc::new(cpu_frame.clone()))
                                    }
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

                        *lock = Some(Arc::new(frame));
                    }
                }
            }
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
                    .map(|v| v.modifiers.iter().map(|v| *v as i64))
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

    let mut obj = pipewire::spa::pod::Object {
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
