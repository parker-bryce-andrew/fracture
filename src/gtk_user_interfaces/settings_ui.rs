use crate::{
    application_channel_creator::UiChannelSide,
    global_application_state::{
        AVAILABLE_PRESETS, FOUND_VERSION, SAFE_MODE, VERSION, load_profiles, profiles_filepath,
        reset_profiles, save_profiles,
    },
    gpu_mirror_display::postprocessing_shaders::DEFAULT_POSTPROCESSOR,
    shaders::{
        SHADER_COLOR_GRADIENT, SHADER_FLIP_HORIZONTAL, SHADER_FLIP_VERTICAL, SHADER_INVERT_COLORS,
        SHADER_ROTATE_LEFT, SHADER_SHOW_ALL_INPUTS,
    },
    ui_state::*,
};
use gtk4::{
    self as gtk, Adjustment, ApplicationWindow, Button, EntryBuffer, TextBuffer, ToggleButton,
    gdk::{Display, RGBA},
    gio::ApplicationFlags,
    glib::{self, ControlFlow, GString},
    prelude::*,
};
use rand::{Rng, seq::IndexedRandom};
use std::{cell::RefCell, rc::Rc, sync::Mutex, time::Duration};
use wgpu::FilterMode;

pub static SETTINGS_IS_RUNNING: Mutex<bool> = Mutex::new(false);

pub fn run_settings_app(channel: &Rc<RefCell<UiChannelSide>>, state: Rc<RefCell<UiState>>) {
    {
        let application = gtk4::Application::builder()
            .flags(ApplicationFlags::NON_UNIQUE)
            .application_id("com.example.FirstGtkApp")
            .build();

        let app_channels = channel.clone();

        application.connect_activate(move |app| {
            let provider = gtk4::CssProvider::new();
            let css = include_str!("../../css/border.css");

            provider.load_from_string(css);

            gtk::style_context_add_provider_for_display(
                &Display::default().expect("Could not connect to a display."),
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );

            let window = ApplicationWindow::builder()
                .application(app)
                .title("🛠️")
                .build();

            let window = Rc::new(window);
            let glib_window = window.clone();

            let system_close_requested = Rc::new(RefCell::new(false));
            let system_shutdown_requested = system_close_requested.clone();

            let glib_shutdown_complete = Rc::new(RefCell::new(false));
            let glib_loop_is_done = glib_shutdown_complete.clone();

            window.connect_close_request(move |_| {
                *system_close_requested.borrow_mut() = true;

                match *&*glib_shutdown_complete.borrow() {
                    true => glib::Propagation::Proceed,
                    _ => glib::Propagation::Stop,
                }
            });

            let app_channels = app_channels.clone();
            let app_channels2 = app_channels.clone();

            // This loop doesn't suspend when gtk is suspended, this solves killing gtk when
            // it's suspended
            glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
                let mut should_kill = false;

                while let Ok(_) = app_channels2.borrow_mut().kill_with_confirm_recv.try_recv() {
                    *glib_loop_is_done.borrow_mut() = true;
                    should_kill = true;
                }

                let sys_req: bool = *system_shutdown_requested.borrow();

                if sys_req {
                    should_kill = true;
                    *glib_loop_is_done.borrow_mut() = true;
                }

                match should_kill {
                    true => {
                        glib_window.close();

                        ControlFlow::Break
                    }
                    false => ControlFlow::Continue,
                }
            });

            window.set_child(Some(&rebuild(&Rc::clone(&state))));

            let state: Rc<RefCell<UiState>> = state.clone();

            let to_move = app_channels.clone();

            window.add_tick_callback(move |window, _| {
                {
                    let to_move: &mut UiChannelSide = &mut *to_move.borrow_mut();

                    while let Ok(recv) = to_move.gpu_receiver_request.try_recv() {
                        let before = { state.borrow().scroll_value.clone() };

                        *state.borrow_mut() = recv;

                        state.borrow_mut().scroll_value = before;
                    }
                }

                {
                    let mut state = state.borrow_mut();

                    if let Some(timer) = state.delayed_uptime_timer.clone() {
                        if let Ok(v) = timer.elapsed() {
                            if Duration::from_secs(1) < v {
                                state.update().delayed_uptime_timer = None;
                            }
                        }
                    }
                }

                let should_rebuild = state.borrow().need_rebuild;
                let should_send_state = state.borrow().updated;

                let ui: &UiChannelSide = &*to_move.borrow();
                let gpu_channel_sender = ui.updated_state_sender.clone();

                if should_send_state {
                    {
                        let temp: &UiState = &state.borrow();
                        let temp: UiState = temp.clone();

                        if let Err(e) = gpu_channel_sender.send(temp) {
                            println!("The receiver for the GPU was dropped: {:?}", e);
                        }
                    }
                    state.borrow_mut().updated = false;
                }

                if should_rebuild {
                    window.set_child(Some(&rebuild(&Rc::clone(&state))));

                    {
                        let temp: &UiState = &state.borrow();
                        let temp: UiState = temp.clone();

                        if let Err(e) = gpu_channel_sender.send(temp) {
                            println!("The sender for the GPU was dropped: {:?}", e);
                        }
                    }

                    state.borrow_mut().need_rebuild = false;
                }

                glib::ControlFlow::Continue
            });

            window.present();
        });

        application.run();
        application.quit();
        // unsafe { application.run_dispose() };
    }
}

pub fn run_settings_ui(mut ui: UiChannelSide) {
    if let Ok(received_stream) = ui.stream_start_check_settings_ui.recv() {
        if !received_stream {
            println!("failed to select stream");

            return;
        }
    } else {
        return;
    }

    let state = Rc::new(RefCell::new(UiState::default()));

    'ui_loop: loop {
        let starter = ui.start_signal_receiver.recv();

        if let Err(e) = starter {
            println!(
                "The Settings UI is stopping because the channel was droppped: {:?}",
                e
            );

            println!("Killing the Settings UI.");

            break 'ui_loop;
        }

        {
            *SETTINGS_IS_RUNNING.lock().unwrap() = true;
        };

        while let Ok(_) = ui.start_signal_receiver.try_recv() {}

        if let Ok(_) = ui.stop_settings_ui.try_recv() {
            println!("Killing the Settings UI.");

            *SETTINGS_IS_RUNNING.lock().unwrap() = false;

            break 'ui_loop;
        }

        let state = state.clone();

        {
            while let Ok(value) = { ui.gpu_receiver_request.try_recv() } {
                {
                    *state.borrow_mut() = value;
                }
            }
        }

        let channels = Rc::new(RefCell::new(ui));

        run_settings_app(&channels, state);

        {
            *SETTINGS_IS_RUNNING.lock().unwrap() = false;
        }

        // let w = Rc::weak_count(&channels);
        // let s = Rc::strong_count(&channels);

        // println!("weak: {}, strong: {}", w, s);

        let t = Rc::into_inner(channels).unwrap();
        let t = RefCell::into_inner(t);

        ui = t;
    }
}

// impl UiState where RefCell<UiState> {
pub fn rebuild(v: &Rc<RefCell<UiState>>) -> gtk::Box {
    let base = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .hexpand(true)
        .halign(gtk4::Align::Center)
        .spacing(10)
        .orientation(gtk4::Orientation::Vertical)
        .build();

    let title_display = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let display_title = gtk::ToggleButton::with_label("TitleBarVisible");
    let hidden = gtk::ToggleButton::with_label("HiddenTitleBar");

    let v2 = v.clone();

    display_title.connect_clicked(move |_| {
        let state = v2.clone();

        *&mut state.borrow_mut().update().display_title = TitleBarDisplay::TitleBarVisible;
    });

    let v2 = v.clone();

    hidden.connect_clicked(move |_| {
        let state = v2.clone();

        *&mut state.borrow_mut().update().display_title = TitleBarDisplay::HiddenTitleBar;
    });

    let v2 = v.clone();

    match v2.borrow().display_title {
        TitleBarDisplay::HiddenTitleBar => hidden.set_active(true),
        TitleBarDisplay::TitleBarVisible => display_title.set_active(true),
    }

    // let title_opt = gtk::Box::builder()
    title_display.append(&display_title);
    title_display.append(&hidden);

    let mag_display = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let nearest = gtk::ToggleButton::with_label("Nearest");
    let linear = gtk::ToggleButton::with_label("Linear");

    let v2 = v.clone();

    nearest.connect_clicked(move |_| {
        let state = v2.clone();

        let mut temp = state.borrow_mut();
        let temp: &mut UiState = temp.update();

        temp.magnify_filter = wgpu::FilterMode::Nearest;
        temp.should_define_new_primary_sampler = true;
    });

    let v2 = v.clone();

    linear.connect_clicked(move |_| {
        let state = v2.clone();

        let mut temp = state.borrow_mut();
        let temp: &mut UiState = temp.update();

        temp.magnify_filter = wgpu::FilterMode::Linear;
        temp.should_define_new_primary_sampler = true;
    });

    let v2 = v.clone();

    match v2.borrow().magnify_filter {
        wgpu::FilterMode::Linear => linear.set_active(true),
        wgpu::FilterMode::Nearest => nearest.set_active(true),
    }

    // let title_opt = gtk::Box::builder()
    mag_display.append(&nearest);
    mag_display.append(&linear);

    let min_display = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let nearest = gtk::ToggleButton::with_label("Nearest");
    let linear = gtk::ToggleButton::with_label("Linear");

    let v2 = v.clone();

    nearest.connect_clicked(move |_| {
        let state = v2.clone();

        let mut temp = state.borrow_mut();
        let temp: &mut UiState = temp.update();

        temp.minify_filter = wgpu::FilterMode::Nearest;
        temp.should_define_new_primary_sampler = true;
    });

    let v2 = v.clone();

    linear.connect_clicked(move |_| {
        let state = v2.clone();

        let mut temp = state.borrow_mut();
        let temp: &mut UiState = temp.update();

        temp.minify_filter = wgpu::FilterMode::Linear;
        temp.should_define_new_primary_sampler = true;
    });

    let v2 = v.clone();

    match v2.borrow().minify_filter {
        wgpu::FilterMode::Linear => linear.set_active(true),
        wgpu::FilterMode::Nearest => nearest.set_active(true),
    }

    min_display.append(&nearest);
    min_display.append(&linear);

    let presents = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let avail = { AVAILABLE_PRESETS.lock().unwrap().clone() };

    for preset in avail.iter() {
        let label = format!("{:?}", preset);

        let btn = gtk::ToggleButton::with_label(&label);
        let v2 = v.clone();

        let pre = preset.clone();

        btn.connect_clicked(move |_| {
            let state = v2.clone();

            let mut temp = state.borrow_mut();
            let temp: &mut UiState = temp.update();

            temp.should_define_new_preset = true;

            temp.preset = pre;
        });

        if { v.borrow().preset } == *preset {
            btn.set_active(true);
        }

        presents.append(&btn);
    }

    let hittest = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let interact = gtk::ToggleButton::with_label("Interactable");
    let pass = gtk::ToggleButton::with_label("PassThrough");

    let v2 = v.clone();

    interact.connect_clicked(move |_| {
        let state = v2.clone();

        let mut temp = state.borrow_mut();
        let temp: &mut UiState = temp.update();

        temp.window_interactions = WindowInteractions::Interactable;
    });

    let v2 = v.clone();

    pass.connect_clicked(move |_| {
        let state: Rc<RefCell<UiState>> = v2.clone();

        let mut temp = state.borrow_mut();
        let temp: &mut UiState = temp.update();

        temp.window_interactions = WindowInteractions::PassThrough;
    });

    let v2 = v.clone();

    match v2.borrow().window_interactions {
        WindowInteractions::Interactable => interact.set_active(true),
        WindowInteractions::PassThrough => pass.set_active(true),
    }

    hittest.append(&interact);
    hittest.append(&pass);

    let aspect_ratio_display = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let maintain_aspect_ratio_btn =
        gtk::ToggleButton::with_label("MaintainAspectRatio(ScaleDecision, WindowBehaviour)");
    let do_not_maintain_ar_btn = gtk::ToggleButton::with_label("DoNotMaintainAspect");

    let v2 = v.clone();

    maintain_aspect_ratio_btn.connect_clicked(move |_| {
        let state = v2.clone();

        *&mut state.borrow_mut().update().aspect_ratio =
            VideoAspect::MaintainAspectRatio(Default::default(), Default::default());
    });

    let v2 = v.clone();

    do_not_maintain_ar_btn.connect_clicked(move |_| {
        let state = v2.clone();

        *&mut state.borrow_mut().update().aspect_ratio = VideoAspect::DoNotMaintainAspect;
    });

    // let v2 = v.clone();

    aspect_ratio_display.append(&maintain_aspect_ratio_btn);

    aspect_ratio_display.append(&do_not_maintain_ar_btn);

    // let btn = ToggleButton::with_label("+")
    let video_aspect_container = gtk::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(10)
        .css_name("border")
        .build();

    match &v.borrow().aspect_ratio {
        VideoAspect::MaintainAspectRatio(scale_dec, window_behaviour) => {
            maintain_aspect_ratio_btn.set_active(true);

            let scale_decision = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .valign(gtk4::Align::Start)
                .spacing(10)
                .build();

            let scale_decision_row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .valign(gtk4::Align::Start)
                .spacing(10)
                .build();

            scale_decision.append(&gtk::Label::new(Some("ScaleDecision")));
            scale_decision.append(&scale_decision_row);

            let scale_it = gtk::ToggleButton::with_label("Scale");

            let v2 = v.clone();

            scale_it.connect_clicked(move |_| {
                let state = v2.clone();

                let wb = {
                    if let VideoAspect::MaintainAspectRatio(
                        _,
                        scale, //WindowBehaviour::SizeSetByUser(_, defined),
                    ) = &state.borrow().aspect_ratio
                    {
                        scale.clone()
                    } else {
                        WindowBehaviour::SizeMatchesMirrorAspect
                    }
                };

                *&mut state.borrow_mut().update().aspect_ratio =
                    VideoAspect::MaintainAspectRatio(ScaleDecision::Scale, wb);
            });

            let dont_scale = gtk::ToggleButton::with_label("DontScale");

            let v2 = v.clone();

            dont_scale.connect_clicked(move |_| {
                let state = v2.clone();

                let wb = {
                    if let VideoAspect::MaintainAspectRatio(
                        _,
                        scale, //WindowBehaviour::SizeSetByUser(_, defined),
                    ) = &state.borrow().aspect_ratio
                    {
                        scale.clone()
                    } else {
                        WindowBehaviour::SizeMatchesMirrorAspect
                    }
                };

                *&mut state.borrow_mut().update().aspect_ratio =
                    VideoAspect::MaintainAspectRatio(ScaleDecision::DontScale, wb);
            });

            scale_decision_row.append(&scale_it);
            scale_decision_row.append(&dont_scale);

            // scale_decision.set_css_classes(&["border"]);
            video_aspect_container.append(&scale_decision);

            match scale_dec {
                ScaleDecision::DontScale => dont_scale.set_active(true),
                ScaleDecision::Scale => scale_it.set_active(true),
            }

            let aspect_choices_r1 = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .valign(gtk4::Align::Start)
                .spacing(10)
                .build();

            let v2 = v.clone();

            let sizes_matches_mirror = gtk::ToggleButton::with_label("SizeMatchesMirrorAspect");
            let size_set_by_user = gtk::ToggleButton::with_label("SizeSetByUser(VideoLocation)");

            sizes_matches_mirror.connect_clicked(move |_| {
                let state = v2.clone();

                let current_scale = {
                    if let VideoAspect::MaintainAspectRatio(
                        scale,
                        _, //WindowBehaviour::SizeSetByUser(_, defined),
                    ) = &state.borrow().aspect_ratio
                    {
                        scale.clone()
                    } else {
                        Default::default()
                    }
                };

                *&mut state.borrow_mut().update().aspect_ratio = VideoAspect::MaintainAspectRatio(
                    current_scale,
                    WindowBehaviour::SizeMatchesMirrorAspect,
                );
            });

            let v2 = v.clone();

            size_set_by_user.connect_clicked(move |_| {
                let state = v2.clone();

                let current_scale = {
                    if let VideoAspect::MaintainAspectRatio(
                        scale,
                        _, //WindowBehaviour::SizeSetByUser(_, defined),
                    ) = &state.borrow().aspect_ratio
                    {
                        scale.clone()
                    } else {
                        Default::default()
                    }
                };

                *&mut state.borrow_mut().update().aspect_ratio = VideoAspect::MaintainAspectRatio(
                    current_scale,
                    WindowBehaviour::SizeSetByUser(VideoLocation::Center),
                );
            });

            aspect_choices_r1.append(&sizes_matches_mirror);
            aspect_choices_r1.append(&size_set_by_user);
            video_aspect_container.add_css_class("border");
            video_aspect_container.append(&gtk::Label::new(Some("WindowBehaviour")));
            video_aspect_container.append(&aspect_choices_r1);

            match window_behaviour {
                WindowBehaviour::SizeMatchesMirrorAspect => {
                    sizes_matches_mirror.set_active(true);
                }
                WindowBehaviour::SizeSetByUser(video_location) => {
                    size_set_by_user.set_active(true);

                    let size_set_by_user_row = gtk::Box::builder()
                        .valign(gtk4::Align::Center)
                        .margin_start(10)
                        .spacing(10)
                        .orientation(gtk4::Orientation::Horizontal)
                        .build();

                    // size_set_by_user.set_data(key, value);
                    // // size_set_by_user.set_css_classes(&["border"]);
                    // unsafe {
                    //     size_set_by_user.set_data(&"border-style", &"solid");
                    //     size_set_by_user.set_data(&"border-width", &"10px");
                    // }
                    // // size_set_by_user.
                    // unsafe {
                    //     size_set_by_user.set_data(&"border-color", &"rgb(83, 11, 11);");
                    //     size_set_by_user.set_data(&"border-width", &"10px;");
                    // }
                    // .style_context()
                    // .set_property("border-color", "rgb(83, 11, 11);"); //.border().set_right(10);//.bind_property("", &"border-color", "rgb(83, 11, 11);");
                    // size_set_by_user.set_layout_manager(layout_manager);

                    let p1 = gtk::Box::builder()
                        .valign(gtk4::Align::Start)
                        .spacing(10)
                        .orientation(gtk4::Orientation::Vertical)
                        .build();

                    p1.append(&gtk::Label::new(Some("VideoLocation")));

                    size_set_by_user_row.set_css_classes(&["border"]);

                    size_set_by_user_row.append(&p1);

                    let btn0 = ToggleButton::with_label("+");
                    let btn1 = ToggleButton::with_label("+");
                    let btn2 = ToggleButton::with_label("+");
                    let btn3 = ToggleButton::with_label("+");
                    let btn4 = ToggleButton::with_label("+");
                    let btn5 = ToggleButton::with_label("+");
                    let btn6 = ToggleButton::with_label("+");
                    let btn7 = ToggleButton::with_label("+");
                    let btn8 = ToggleButton::with_label("+");

                    let buttons = vec![
                        &btn0, &btn1, &btn2, &btn3, &btn4, &btn5, &btn6, &btn7, &btn8,
                    ];

                    for (idx, btn) in buttons.iter().enumerate() {
                        let v2 = v.clone();

                        btn.connect_clicked(move |_| {
                            let state = v2.clone();

                            let current_scale = {
                                if let VideoAspect::MaintainAspectRatio(
                                    scale,
                                    _, //WindowBehaviour::SizeSetByUser(_, defined),
                                ) = &state.borrow().aspect_ratio
                                {
                                    scale.clone()
                                } else {
                                    Default::default()
                                }
                            };

                            *&mut state.borrow_mut().update().aspect_ratio =
                                VideoAspect::MaintainAspectRatio(
                                    current_scale,
                                    WindowBehaviour::SizeSetByUser(
                                        (idx as i32).into(),
                                        // color_state,
                                    ),
                                );
                        });
                    }

                    let mirror_orientation_r1 = gtk::Box::builder()
                        .orientation(gtk::Orientation::Horizontal)
                        .valign(gtk4::Align::Start)
                        .build();

                    mirror_orientation_r1.append(buttons[0]);
                    mirror_orientation_r1.append(buttons[1]);
                    mirror_orientation_r1.append(buttons[2]);

                    let mirror_orientation_r2 = gtk::Box::builder()
                        .orientation(gtk::Orientation::Horizontal)
                        .valign(gtk4::Align::Start)
                        .build();

                    mirror_orientation_r2.append(buttons[3]);
                    mirror_orientation_r2.append(buttons[4]);
                    mirror_orientation_r2.append(buttons[5]);

                    let mirror_orientation_r3 = gtk::Box::builder()
                        .orientation(gtk::Orientation::Horizontal)
                        .valign(gtk4::Align::Start)
                        .build();

                    mirror_orientation_r3.append(buttons[6]);
                    mirror_orientation_r3.append(buttons[7]);
                    mirror_orientation_r3.append(buttons[8]);

                    let mirror_orientation = gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .valign(gtk::Align::Start)
                        .build();

                    mirror_orientation.append(&mirror_orientation_r1);
                    mirror_orientation.append(&mirror_orientation_r2);
                    mirror_orientation.append(&mirror_orientation_r3);

                    p1.append(&mirror_orientation);
                    video_aspect_container.append(&size_set_by_user_row);
                    buttons[video_location.clone() as usize].set_active(true);
                }
            }
        }
        VideoAspect::DoNotMaintainAspect => {
            do_not_maintain_ar_btn.set_active(true);
        }
    }

    // base.append(&gtk::Label::new("Links".into()));

    // let v2 = v.clone();

    let info = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        // .halign(gtk4::Align::Center)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let test = gtk::LinkButton::builder()
        .label("Source")
        .uri("https://github.com/parker-bryce-andrew/fracture")
        .build();

    info.append(&test);

    const FRACTURE_LINK: &'static str = "https://programming.dev/c/fracture";

    let test = gtk::LinkButton::builder()
        .label("/c/Fracture")
        .uri(FRACTURE_LINK)
        .build();

    info.append(&test);

    let test = gtk::LinkButton::builder()
        .label("Mastodon")
        .uri("https://sigmoid.social/@parker")
        .build();

    info.append(&test);

    let test = gtk::LinkButton::builder()
        .label("Socials")
        .uri("https://parker.andrew.cx")
        .build();

    info.append(&test);

    let should_warn = VERSION.to_string() != FOUND_VERSION.to_string();

    let version_text = format!("V: {}", VERSION);
    let update_text = format!("update available: {}", FOUND_VERSION.to_string());

    let version = gtk::Label::builder()
        .label(&version_text)
        .tooltip_text(&update_text)
        .build();

    version.set_has_tooltip(should_warn);

    if should_warn {
        version.add_css_class("warn");
        version.add_css_class("pad_text");
    }

    info.append(&version);

    base.append(&info);

    base.append(&gtk::Label::new("TitleBarDisplay".into()));
    base.append(&title_display);

    let greenscreen_content = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let greenscreen_display = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let no_green_screen = gtk::ToggleButton::with_label("None");
    let use_green_screen = gtk::ToggleButton::with_label("Color(RemoveColors)");

    let v2 = v.clone();

    no_green_screen.connect_clicked(move |_| {
        let state = v2.clone();

        *&mut state.borrow_mut().update().green_screen = GreenScreen::None;
    });

    let v2 = v.clone();

    use_green_screen.connect_clicked(move |_| {
        let state = v2.clone();

        *&mut state.borrow_mut().update().green_screen = GreenScreen::Color(RemoveColors {
            base_color: (0.0, 0.0, 0.0),
            sensitivity: (0.0),
        });
    });

    let v2 = v.clone();

    match &v2.borrow().green_screen {
        GreenScreen::None => no_green_screen.set_active(true),
        GreenScreen::Color(RemoveColors {
            base_color: (r, g, b),
            sensitivity,
        }) => {
            use_green_screen.set_active(true);

            let remove_colors_container = gtk::Box::builder()
                .valign(gtk4::Align::Start)
                .spacing(10)
                .orientation(gtk4::Orientation::Horizontal)
                .build();

            let v2 = v.clone();

            #[allow(deprecated)]
            let color_choice = gtk::ColorChooserWidget::builder()
                .show_editor(true)
                .rgba(&RGBA::new(*r, *g, *b, 0.0))
                .build();

            #[allow(deprecated)]
            color_choice.connect_rgba_notify(move |widget| {
                let new_color = &widget.rgba();

                let state = v2.clone();

                let sense = if let GreenScreen::Color(RemoveColors {
                    base_color: _,
                    sensitivity,
                }) = { state.borrow().green_screen.clone() }
                {
                    sensitivity
                } else {
                    0.0
                };

                *&mut state.borrow_mut().update_delayed_rebuild().green_screen =
                    GreenScreen::Color(RemoveColors {
                        base_color: (new_color.red(), new_color.green(), new_color.blue()),
                        sensitivity: sense,
                    });
            });

            // color_choice.add_css_class("border");

            remove_colors_container.add_css_class("border");

            let sensitivity_scale = gtk::Scale::builder()
                .orientation(gtk4::Orientation::Horizontal)
                .adjustment(&Adjustment::builder().lower(0.0).upper(100.0).build())
                .build();

            sensitivity_scale.set_value(*sensitivity as f64);
            let v2 = v.clone();

            sensitivity_scale.connect_value_changed(move |v| {
                let v = v.value() as f32;
                let state = v2.clone();

                let rgb = if let GreenScreen::Color(RemoveColors {
                    base_color,
                    sensitivity: _,
                }) = { state.borrow().green_screen.clone() }
                {
                    base_color
                } else {
                    (0.0, 0.0, 0.0)
                };

                state.borrow_mut().update_delayed_rebuild().green_screen =
                    GreenScreen::Color(RemoveColors {
                        base_color: rgb,
                        sensitivity: v,
                    });
            });

            let p1 = gtk::Box::builder()
                .valign(gtk4::Align::Start)
                .spacing(10)
                .orientation(gtk4::Orientation::Vertical)
                .build();

            p1.append(&gtk::Label::new("sensitivity".into()));
            p1.append(&sensitivity_scale);

            p1.set_width_request(150);

            let p2 = gtk::Box::builder()
                .valign(gtk4::Align::Start)
                .spacing(10)
                .orientation(gtk4::Orientation::Vertical)
                .build();

            p2.append(&gtk::Label::new("base_color".into()));
            p2.append(&color_choice);

            remove_colors_container.append(&p1);
            remove_colors_container.append(&p2);

            greenscreen_content.append(&remove_colors_container);
        }
    }

    // let title_opt = gtk::Box::builder()
    greenscreen_display.append(&no_green_screen);
    greenscreen_display.append(&use_green_screen);

    let postprocessor_display = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let no_postprocess = gtk::ToggleButton::with_label("None");
    let use_postprocess = gtk::ToggleButton::with_label("Postprocessor(Wgsl)");

    let v2 = v.clone();

    no_postprocess.connect_clicked(move |_| {
        let state = v2.clone();

        let temp = &mut state.borrow_mut();
        let temp = temp.update();

        temp.postprocessor = None;
        temp.gpu_requested_compile = true;
    });

    let v2 = v.clone();

    use_postprocess.connect_clicked(move |_| {
        let state = v2.clone();

        let example: String = DEFAULT_POSTPROCESSOR.into();
        let example = example.replace("\\t", "\t");

        let temp = &mut state.borrow_mut();
        let temp = temp.update();

        temp.gpu_requested_compile = true;

        temp.postprocessor = Some(Postprocessor {
            submitted_postprocessor: Some(example.clone().into()),

            editing_postprocessor: example.into(),
            last_errors: None,
        });
    });

    postprocessor_display.append(&no_postprocess);
    postprocessor_display.append(&use_postprocess);

    let postprocessor_content = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Vertical)
        .build();

    postprocessor_content.add_css_class("border");

    let v2 = v.clone();

    match &v2.borrow().postprocessor {
        Some(processor) => {
            use_postprocess.set_active(true);

            let buff = gtk::TextBuffer::builder()
                .text(&format!(
                    "{}",
                    &processor.editing_postprocessor.replace("    ", "\t").trim()
                ))
                .build();

            let v2 = v.clone();

            buff.connect_changed(move |v| {
                let data: GString = v.text(&v.start_iter(), &v.end_iter(), false);

                let after = format!("{}", data);

                let posty = v2.borrow().postprocessor.clone();

                if let Some(mut post) = posty {
                    post.editing_postprocessor = after;

                    v2.borrow_mut().postprocessor = Some(post);
                } else {
                    v2.borrow_mut().postprocessor = Some(Postprocessor {
                        submitted_postprocessor: None,
                        // gpu_requested_to_compile_submitted: false,
                        editing_postprocessor: after,
                        last_errors: None,
                    });
                }
            });

            let has_err = {
                if let Some(p) = &v.borrow().postprocessor {
                    if let Some(_) = p.last_errors {
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            let text = gtk::TextView::builder().buffer(&buff).build();

            if has_err {
                text.add_css_class("err");
            }

            postprocessor_content.append(&text);

            if has_err {
                let err_text = if let Some(Some(err)) =
                    &v.borrow().postprocessor.as_ref().map(|v| &v.last_errors)
                {
                    format!("{:#?}", err)
                } else {
                    "".into()
                };

                let buff = gtk::TextBuffer::builder()
                    .text(&format!("{}", &err_text.replace("    ", "\t").trim()))
                    .build();

                let text = gtk::TextView::builder()
                    .buffer(&buff)
                    .editable(false)
                    .build();

                postprocessor_content.append(&text);
            }

            let btn = gtk::Button::builder()
                // .hexpand(true)
                .label("Compile")
                .build();

            let v2 = v.clone();

            btn.connect_clicked(move |_| {
                let mut temp = v2.borrow_mut();
                let temp = temp.update();

                temp.gpu_requested_compile = true;

                if let Some(post) = &mut temp.postprocessor {
                    let temp = post.editing_postprocessor.clone();
                    let temp = temp.trim();

                    post.submitted_postprocessor = Some(temp.into());
                }
            });

            let compile_btn_row = gtk::Box::builder()
                .valign(gtk4::Align::Start)
                // .hexpand(true)
                // .vexpand(true)
                .spacing(10)
                .orientation(gtk4::Orientation::Horizontal)
                .build();

            compile_btn_row.append(&btn);

            let more_shaders = gtk::Button::builder()
                .label("Find shaders online")
                // .hexpand(true)
                .build();

            more_shaders.connect_clicked(|_| {
                let _ = open::that(FRACTURE_LINK);
            });

            compile_btn_row.append(&more_shaders);

            let temp = gtk::DropDown::from_strings(&[
                "Make a selection ",
                "Invert colors",
                "Flip horizontal",
                "Flip vertical",
                "Shifting color gradient overlay",
                "Rotate left",
                "Show all inputs (Displays the entire shader with it commented out)",
            ]);

            let v2 = v.clone();

            temp.connect_selected_item_notify(move |v| {
                let temp = v.selected();

                let shader = match temp {
                    0 => {
                        return;
                    }
                    1 => SHADER_INVERT_COLORS,
                    2 => SHADER_FLIP_HORIZONTAL,
                    3 => SHADER_FLIP_VERTICAL,
                    4 => SHADER_COLOR_GRADIENT,
                    5 => SHADER_ROTATE_LEFT,
                    6 => SHADER_SHOW_ALL_INPUTS,
                    _ => "",
                };

                let mut temp = v2.borrow_mut();
                let temp = temp.update();

                temp.postprocessor = Some(Postprocessor {
                    submitted_postprocessor: Some(shader.into()),
                    editing_postprocessor: shader.into(),
                    last_errors: None,
                });

                temp.gpu_requested_compile = true;
            });

            compile_btn_row.append(&gtk::Label::new("Example".into()));
            compile_btn_row.append(&temp);

            postprocessor_content.append(&compile_btn_row);
        }
        None => {
            no_postprocess.set_active(true);
        }
    }

    base.append(&gtk::Label::new("VideoAspect".into()));

    base.append(&aspect_ratio_display);

    let text = gtk::TextView::builder()
        .editable(false)
        .buffer(&{
            let temp = v.borrow().clone();

            let temp = temp.lossy_into_set_ui();

            let text = format!("{:#?}", &temp);

            gtk::TextBuffer::builder().text(&text).build()
        })
        .build();

    base.append(&video_aspect_container);

    base.append(&gtk::Label::new("frame_transparency".into()));

    let scale = gtk::Scale::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .adjustment(&Adjustment::builder().lower(0.0).upper(100.0).build())
        .build();

    scale.set_value(v.borrow().frame_transparency as f64);
    let v2 = v.clone();

    scale.connect_value_changed(move |v| {
        let v = v.value() as f32;
        let state = v2.clone();

        state
            .borrow_mut()
            .update_delayed_rebuild()
            .frame_transparency = v;
    });

    let all = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .propagate_natural_width(true)
        .build();

    base.append(&scale);

    base.append(&gtk::Label::new("GreenScreen".into()));
    base.append(&greenscreen_display);
    base.append(&greenscreen_content);

    let background_content = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let background_display = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let v2 = v.clone();

    let transparent = gtk::ToggleButton::with_label("Transparent");
    let color = gtk::ToggleButton::with_label("Color(f32, f32, f32, f32)");

    transparent.connect_clicked(move |_| {
        let state = v2.clone();

        *&mut state.borrow_mut().update().background = WindowBackground::Transparent;
    });

    let v2 = v.clone();

    color.connect_clicked(move |_| {
        let state = v2.clone();

        *&mut state.borrow_mut().update().background = WindowBackground::default();
    });

    background_display.append(&transparent);
    background_display.append(&color);

    match &v.borrow().background {
        WindowBackground::Transparent => transparent.set_active(true),
        WindowBackground::Color(r, g, b, a) => {
            color.set_active(true);

            let v2 = v.clone();

            #[allow(deprecated)]
            let color_choice = gtk::ColorChooserWidget::builder()
                .show_editor(true)
                .rgba(&RGBA::new(*r, *g, *b, *a))
                .build(); //.accessible_role(gtk4::AccessibleRole::Grid).build();

            #[allow(deprecated)]
            color_choice.connect_rgba_notify(move |widget| {
                let new_color = &widget.rgba();

                let state = v2.clone();

                *&mut state.borrow_mut().update_delayed_rebuild().background =
                    WindowBackground::Color(
                        new_color.red(),
                        new_color.green(),
                        new_color.blue(),
                        new_color.alpha(),
                    )
            });

            color_choice.add_css_class("border");

            background_content.append(&color_choice);
        }
    }

    base.append(&gtk::Label::new("window_background".into()));
    base.append(&background_display);
    base.append(&background_content);

    base.append(&gtk::Label::new("Postprocessor (Custom Shader)".into()));
    base.append(&postprocessor_display);
    base.append(&postprocessor_content);

    base.append(&gtk::Label::new("magnify_filter".into()));
    base.append(&mag_display);

    base.append(&gtk::Label::new("minify_filter".into()));
    base.append(&min_display);

    base.append(&gtk::Label::new("WindowInteractions".into()));
    base.append(&hittest);

    base.append(&gtk::Label::new("Presents".into()));
    base.append(&presents);

    if let Err(_) = std::env::var(SAFE_MODE) {
        base.append(&gtk::Label::new("Safe Mode".into()));

        let safe_mode_display_box = gtk::Box::builder()
            .valign(gtk4::Align::Start)
            .spacing(10)
            .orientation(gtk4::Orientation::Horizontal)
            .build();

        let restart_btn = gtk::Button::builder().label("Restart in safe mode").build();

        safe_mode_display_box.append(&restart_btn);

        restart_btn.connect_clicked(move |_| {
            let text = &gtk::Label::new("Restart in safe mode?".into());

            let display = gtk::Box::builder()
                .valign(gtk4::Align::Start)
                .spacing(10)
                .margin_bottom(10)
                .margin_end(10)
                .margin_start(10)
                .margin_top(10)
                .orientation(gtk4::Orientation::Vertical)
                .build();

            display.append(text);

            let dia_safe_mode_btn_box = gtk::Box::builder()
                .valign(gtk4::Align::Start)
                .spacing(5)
                .orientation(gtk4::Orientation::Vertical)
                .build();

            let safe_mode_yes_dia = gtk::Button::builder().label("Yes").build();
            let safe_mode_no_dia = gtk::Button::builder().label("No").build();

            dia_safe_mode_btn_box.append(&safe_mode_yes_dia);
            dia_safe_mode_btn_box.append(&safe_mode_no_dia);

            display.append(&dia_safe_mode_btn_box);

            #[allow(deprecated)]
            let safe_mode_dia_box = gtk::Dialog::builder().title("").child(&display).build();

            #[allow(deprecated)]
            safe_mode_dia_box.show();

            safe_mode_yes_dia.connect_clicked(move |_| {
                println!("Attempting to restart to SAFE_MODE");

                std::process::abort();
            });

            safe_mode_no_dia.connect_clicked(move |_| {
                safe_mode_dia_box.close();
            });
        });

        base.append(&safe_mode_display_box);
    }

    base.append(&gtk::Label::new("[Debug] SetUiState".into()));
    let force_update = Button::with_label("Check SetUiState");

    let v2 = v.clone();
    force_update.connect_clicked(move |_| {
        v2.clone().borrow_mut().update();
    });

    base.append(&force_update);

    base.append(&text);

    let export_import_display = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let (export, import, randomizer) = (
        gtk::Button::builder().label("Export").build(),
        gtk::Button::builder().label("Import").build(),
        gtk::Button::builder().label("Randomize").build(),
    );

    let v2 = v.clone();

    randomizer.connect_clicked(move |_| {
        let mut rng = rand::rng();

        let temp = SetUiState {
            display_title: vec![
                TitleBarDisplay::HiddenTitleBar,
                TitleBarDisplay::TitleBarVisible,
            ]
            .choose(&mut rng)
            .unwrap()
            .clone(),
            aspect_ratio: if rng.random_bool(0.8) {
                VideoAspect::MaintainAspectRatio(
                    vec![ScaleDecision::Scale, ScaleDecision::DontScale]
                        .choose(&mut rng)
                        .unwrap()
                        .clone(),
                    vec![
                        WindowBehaviour::SizeMatchesMirrorAspect,
                        WindowBehaviour::SizeSetByUser(
                            vec![
                                VideoLocation::NorthWest,
                                VideoLocation::North,
                                VideoLocation::NorthEast,
                                VideoLocation::West,
                                VideoLocation::Center,
                                VideoLocation::East,
                                VideoLocation::SouthWest,
                                VideoLocation::South,
                                VideoLocation::SouthEast,
                                VideoLocation::Center,
                            ]
                            .choose(&mut rng)
                            .unwrap()
                            .clone(),
                        ),
                    ]
                    .choose(&mut rng)
                    .unwrap()
                    .clone(),
                )
            } else {
                VideoAspect::DoNotMaintainAspect
            },
            frame_transparency: if rng.random_bool(0.5) {
                rng.sample(rand::distr::Uniform::new(20.0, 100.0).unwrap())
            } else {
                100.0
            },
            green_screen: if rng.random_bool(0.5) {
                GreenScreen::None
            } else {
                GreenScreen::Color(RemoveColors {
                    base_color: (
                        rng.sample(rand::distr::Uniform::new(0.0, 1.0).unwrap()),
                        rng.sample(rand::distr::Uniform::new(0.0, 1.0).unwrap()),
                        rng.sample(rand::distr::Uniform::new(0.0, 1.0).unwrap()),
                    ),
                    sensitivity: rng.sample(rand::distr::Uniform::new(0.0, 50.0).unwrap()),
                })
            },
            window_background: if rng.random_bool(0.5) {
                WindowBackground::Transparent
            } else {
                WindowBackground::Color(
                    rng.sample(rand::distr::Uniform::new(0.0, 1.0).unwrap()),
                    rng.sample(rand::distr::Uniform::new(0.0, 1.0).unwrap()),
                    rng.sample(rand::distr::Uniform::new(0.0, 1.0).unwrap()),
                    rng.sample(rand::distr::Uniform::new(0.0, 1.0).unwrap()),
                )
            },
            postprocessor: Some({
                let choice: Vec<&str> = vec![
                    SHADER_COLOR_GRADIENT,
                    SHADER_FLIP_HORIZONTAL,
                    SHADER_FLIP_VERTICAL,
                    SHADER_INVERT_COLORS,
                    SHADER_ROTATE_LEFT,
                    SHADER_SHOW_ALL_INPUTS,
                ];

                let select = choice.choose(&mut rng).unwrap().to_string();

                let temp = Postprocessor {
                    submitted_postprocessor: Some(select.clone()),
                    editing_postprocessor: select,
                    last_errors: None,
                };

                temp
            }),
            magnify_filter: if rng.random_bool(0.5) {
                FilterMode::Nearest
            } else {
                FilterMode::Linear
            },
            minify_filter: if rng.random_bool(0.5) {
                FilterMode::Nearest
            } else {
                FilterMode::Linear
            },
            window_interactions: WindowInteractions::Interactable,
            present: Default::default(),
        };

        let mut result = temp.build_new_full_settings_state();

        result.update_delayed_rebuild();
        result.need_rebuild = false;

        result.active_profile = v2.borrow().active_profile;

        *v2.borrow_mut().update_delayed_rebuild() = result;
    });

    let v2 = v.clone();

    export.connect_clicked(move |_| {
        let state = v2.borrow().clone();

        let display = gtk::Box::builder()
            .valign(gtk4::Align::Start)
            .spacing(10)
            .margin_bottom(10)
            .margin_end(10)
            .margin_start(10)
            .margin_top(10)
            .orientation(gtk4::Orientation::Vertical)
            .build();

        let text = gtk::TextView::builder()
            .buffer(&{
                let temp = state;
                let temp = temp.lossy_into_set_ui();
                let create_version: CreateUiState = temp.into();

                let text = serde_json::to_string_pretty(&create_version).unwrap();

                gtk::TextBuffer::builder().text(&text).build()
            })
            .build();

        display.append(&text);

        display.set_width_request(1920 / 3);
        display.set_height_request(1080 / 2);

        #[allow(deprecated)]
        let export_dia = gtk::Dialog::builder()
            .title("Export")
            .child(&display)
            .build();

        #[allow(deprecated)]
        export_dia.show();
    });

    let v2 = v.clone();

    import.connect_clicked(move |_| {
        // let state = v2.borrow().clone();

        let text = gtk::TextView::builder().build();

        text.set_width_request(1920 / 3);
        text.set_height_request(1080 / 2);

        let display = gtk::Box::builder()
            .valign(gtk4::Align::Start)
            .spacing(10)
            .margin_bottom(10)
            .margin_end(10)
            .margin_start(10)
            .margin_top(10)
            .orientation(gtk4::Orientation::Vertical)
            .build();

        display.append(&text);

        let err_text_box = gtk::TextView::builder().visible(false).build();
        let import_confirm = gtk::Button::builder().label("Import").build();

        display.append(&import_confirm);
        display.append(&err_text_box);

        #[allow(deprecated)]
        let import_dia = gtk::Dialog::builder()
            .title("Import")
            .child(&display)
            .build();

        #[allow(deprecated)]
        import_dia.show();

        let v2 = v2.clone();

        import_confirm.connect_clicked(move |_| {
            let buff = text.buffer();
            let data: GString = buff.text(&buff.start_iter(), &buff.end_iter(), false);
            let after: String = format!("{}", data);

            let parse_result = serde_json::from_str::<CreateUiState>(&after);

            if let Ok(parsed) = parse_result {
                import_dia.close();

                let set: SetUiState = parsed.into();
                let new = set.build_new_full_settings_state();
                *v2.borrow_mut().update() = new;
            } else {
                let err_text = parse_result.unwrap_err();
                let err_text = format!("{:#?}", err_text);

                let text_buff = TextBuffer::builder().text(err_text).build();
                err_text_box.set_buffer(Some(&text_buff));
                err_text_box.set_visible(true);

                text.add_css_class("err");
            }
        });
    });

    export_import_display.append(&export);
    export_import_display.append(&import);
    export_import_display.append(&randomizer);

    base.append(&export_import_display);

    base.append(&gtk::Label::new("Profiles".into()));

    let profile_box = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Vertical)
        .build();

    let profile_box_r0 = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let active_profile_out = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Vertical)
        .build();

    let active_profile = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .margin_bottom(10)
        .margin_end(10)
        .margin_start(10)
        .margin_top(10)
        .orientation(gtk4::Orientation::Vertical)
        .build();

    let profile_box_r1 = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let profile_box_r3 = gtk::Box::builder()
        .valign(gtk4::Align::Start)
        .spacing(10)
        .orientation(gtk4::Orientation::Horizontal)
        .build();

    let err_text_box = gtk::TextView::builder().visible(false).build();
    profile_box.append(&profile_box_r0);
    profile_box.append(&err_text_box);

    active_profile_out.append(&active_profile);

    profile_box.append(&active_profile_out);
    active_profile.append(&profile_box_r1);
    active_profile.append(&profile_box_r3);

    let profiles_loaded = load_profiles();

    match profiles_loaded {
        Ok(profiles) => {
            let profile_names: Vec<(usize, String)> = profiles
                .list()
                .iter()
                .map(|v| {
                    v.clone()
                        .name
                        .clone()
                        .map(|v| v.to_string())
                        .unwrap_or("New Profile".into())
                })
                .enumerate()
                .collect();

            let temp: Vec<&str> = profile_names.iter().map(|v| v.1.as_str()).collect();
            let temp2 = gtk::DropDown::from_strings(&temp);

            let selected_idx = { v.borrow().active_profile } as u32;

            temp2.set_selected(selected_idx);

            let selected_profile = profiles.get(selected_idx as usize);

            let selected = selected_profile.config.clone();

            let selected_copy = selected.clone();
            let active: CreateUiState = { v.borrow().clone() }.lossy_into_set_ui().into();

            // active.present = None;
            // selected_copy.present = None;

            if active != selected_copy {
                active_profile_out.add_css_class("warn");

                let debug = format!(
                    "{}{}{}",
                    "The profile below does not match the active configuration.",
                    "\r\n\r\n",
                    "Would you like to save it to the profile?"
                );

                let text_buff = TextBuffer::builder().text(debug).build();
                err_text_box.set_buffer(Some(&text_buff));
                err_text_box.set_visible(true);
                // err_text_box.add_css_class("warn");
            } else {
                active_profile_out.add_css_class("ok");
            }

            let new = gtk::Button::builder().label("New").build();

            let v2 = v.clone();

            new.connect_clicked(move |_| {
                let mut profiles = load_profiles().unwrap_or(Default::default());

                profiles.profiles.push(Default::default());

                let temp = profiles.clone();

                match save_profiles(profiles) {
                    Ok(_) => {
                        let mut v2 = v2.borrow_mut();
                        let state: &mut UiState = v2.update();

                        state.reload_profiles = true;
                        state.active_profile = temp.profiles.len() - 1;
                    }
                    Err(e) => {
                        println!("{:#?}", e);
                        return;
                    }
                }
            });

            profile_box_r0.append(&new);
            profile_box_r0.append(&temp2);

            profile_box_r1.append(&gtk::Label::new("Name".into()));

            let none_text = "None".into();

            let temp = selected_profile
                .name
                .as_ref()
                .unwrap_or(&none_text)
                .as_str();

            let field_val = EntryBuffer::builder().text(temp).build();

            let name_entry = gtk::Entry::builder()
                .name("Name")
                .buffer(&field_val)
                .build();

            profile_box_r1.append(&name_entry);

            let ord = format!("{}", selected_idx);

            let field_val = EntryBuffer::builder().text(&ord).build();

            let entry = gtk::Entry::builder()
                .name("Order")
                .sensitive(false)
                .max_width_chars(3)
                .buffer(&field_val)
                .build();

            profile_box_r1.append(&gtk::Label::new("Order".into()));
            profile_box_r1.append(&entry);

            let up = gtk::Button::builder().label("+").build();

            let v2 = v.clone();

            up.connect_clicked(move |_| {
                let mut profiles = load_profiles().unwrap_or(Default::default());

                let idx = (((selected_idx as isize) + 1) as usize)
                    .min(((profiles.profiles.len() as isize) - 1).max(0) as usize);

                if profiles.profiles.get(selected_idx as usize).is_some() {
                    let temp = profiles.profiles.remove(selected_idx as usize);

                    let idx = idx;
                    profiles.profiles.insert(idx as usize, temp);
                } else {
                    println!("error loading current profile at idx. attempting to reload profiles");
                }

                match save_profiles(profiles) {
                    Ok(_) => {
                        let mut v2 = v2.borrow_mut();
                        let state: &mut UiState = v2.update();

                        state.reload_profiles = true;
                        state.active_profile = idx as usize;
                    }
                    Err(e) => {
                        println!("{:#?}", e);
                        return;
                    }
                }
            });

            let down = gtk::Button::builder().label("-").build();

            let v2 = v.clone();

            down.connect_clicked(move |_| {
                let mut profiles = load_profiles().unwrap_or(Default::default());

                let idx = ((selected_idx as isize) - 1).max(0);

                if profiles.profiles.get(selected_idx as usize).is_some() {
                    let temp = profiles.profiles.remove(selected_idx as usize);

                    let idx = idx;
                    profiles.profiles.insert(idx as usize, temp);
                } else {
                    println!("error loading current profile at idx. attempting to reload profiles");
                }

                match save_profiles(profiles) {
                    Ok(_) => {
                        let mut v2 = v2.borrow_mut();
                        let state: &mut UiState = v2.update();

                        state.reload_profiles = true;
                        state.active_profile = idx as usize;
                    }
                    Err(e) => {
                        println!("{:#?}", e);
                        return;
                    }
                }
            });

            profile_box_r1.append(&down);
            profile_box_r1.append(&up);

            let load = gtk::Button::builder().label("Load").build();

            let v2 = v.clone();

            load.connect_clicked(move |_| {
                let v2 = v2.clone();
                let profiles = load_profiles().unwrap_or(Default::default());
                let using_profile = profiles.get(selected_idx as usize);

                let idx = selected_idx;

                let mut temp = v2.borrow_mut();
                let temp = temp.update();

                *temp = using_profile.config.clone().into();

                temp.reload_profiles = true;
                temp.active_profile = idx as usize;
            });

            let save = gtk::Button::builder().label("Save").build();

            let v2 = v.clone();

            save.connect_clicked(move |_| {
                let v2 = v2.clone();

                if let Ok(mut loaded) = load_profiles() {
                    match loaded.profiles.get_mut(selected_idx as usize) {
                        Some(item) => {
                            let temp: &mut Profile = item;

                            let data: GString = name_entry.buffer().text();
                            let new_name = format!("{}", data);

                            temp.name = Some(new_name);

                            let state = v2.borrow().clone();
                            let to_save: CreateUiState = state.lossy_into_set_ui().into();
                            temp.config = to_save;
                        }
                        None => {
                            println!("failed to index loaded profiles");
                        }
                    }

                    match save_profiles(loaded) {
                        Ok(_) => {
                            let mut v2 = v2.borrow_mut();
                            let state: &mut UiState = v2.update();

                            state.reload_profiles = true;
                        }
                        Err(e) => {
                            println!("{:#?}", e);
                            return;
                        }
                    }
                } else {
                    println!("failed to load profiles");

                    return;
                }

                let profiles = load_profiles().unwrap_or(Default::default());
                let using_profile = profiles.get(selected_idx as usize);

                let idx = selected_idx;

                let mut temp = v2.borrow_mut();
                let temp = temp.update();

                *temp = using_profile.config.clone().into();

                temp.reload_profiles = true;
                temp.active_profile = idx as usize;
            });

            let def = gtk::Button::builder().label("Make default").build();

            let v2 = v.clone();

            def.connect_clicked(move |_| {
                let mut profiles = load_profiles().unwrap_or(Default::default());

                if profiles.profiles.get(selected_idx as usize).is_some() {
                    let temp = profiles.profiles.remove(selected_idx as usize);
                    profiles.profiles.insert(0, temp);
                } else {
                    println!("error loading current profile at idx. attempting to reload profiles");
                }

                match save_profiles(profiles) {
                    Ok(_) => {
                        let mut v2 = v2.borrow_mut();
                        let state: &mut UiState = v2.update();

                        state.reload_profiles = true;
                        state.active_profile = 0
                    }
                    Err(e) => {
                        println!("{:#?}", e);
                        return;
                    }
                }
            });

            let rm = gtk::Button::builder().label("Delete").build();

            let v2 = v.clone();

            rm.connect_clicked(move |_| {
                let mut profiles = load_profiles().unwrap_or(Default::default());

                if profiles.profiles.get(selected_idx as usize).is_some() {
                    let _deleted = profiles.profiles.remove(selected_idx as usize);
                } else {
                    println!("error loading current profile at idx. attempting to reload profiles");
                }

                let copy = profiles.clone();

                match save_profiles(profiles) {
                    Ok(_) => {
                        let mut v2 = v2.borrow_mut();
                        let state: &mut UiState = v2.update();

                        state.reload_profiles = true;

                        let closest = ((selected_idx as isize - 1).max(0))
                            .min((copy.profiles.len() as isize - 1).max(0));

                        state.active_profile = closest as usize;
                    }
                    Err(e) => {
                        println!("{:#?}", e);
                        return;
                    }
                }
            });

            profile_box_r3.append(&load);
            profile_box_r3.append(&save);
            profile_box_r3.append(&def);
            profile_box_r3.append(&rm);

            let v2 = v.clone();

            temp2.connect_selected_item_notify(move |v| {
                let temp_sel = v.selected();

                let mut temp = v2.borrow_mut();
                let temp = temp.update();

                temp.reload_profiles = true;
                temp.active_profile = temp_sel as usize;
            });
        }
        Err(text) => {
            let error_msg: Result<(), _> = Err(text);

            let debug = format!(
                "{}\r\n\r\n{:#?}\r\n\r\n{}\r\n\r\n{:#?}\r\n\r\n{}",
                "The file for profiles failed to load.",
                error_msg,
                "It's located at",
                profiles_filepath(),
                "If you'd like, we can try deleting it?",
            );

            let text_buff = TextBuffer::builder().text(debug).build();
            err_text_box.set_buffer(Some(&text_buff));
            err_text_box.set_visible(true);
            err_text_box.add_css_class("err");

            active_profile_out.set_visible(false);
            profile_box_r0.set_visible(false);

            let reload = gtk::Button::builder().label("Try reloading").build();

            let v2 = v.clone();

            reload.connect_clicked(move |_| match load_profiles() {
                Ok(_) => {
                    let mut v2 = v2.borrow_mut();
                    let state: &mut UiState = v2.update();

                    state.reload_profiles = true;
                    state.active_profile = 0;
                }
                Err(e) => {
                    println!("{:#?}", e);
                    return;
                }
            });

            let delete = gtk::Button::builder().label("Delete it").build();

            let v2 = v.clone();

            delete.connect_clicked(move |_| {
                let profiles = load_profiles().unwrap_or(Default::default());

                match reset_profiles(profiles) {
                    Ok(_) => {
                        let mut v2 = v2.borrow_mut();
                        let state: &mut UiState = v2.update();

                        state.reload_profiles = true;
                        state.active_profile = 0;
                    }
                    Err(e) => {
                        println!("{:#?}", e);
                        return;
                    }
                }
            });

            profile_box.append(&reload);
            profile_box.append(&delete);
        }
    }

    base.append(&profile_box);

    base.set_margin_start(10);
    base.set_margin_top(10);
    base.set_margin_bottom(10);
    base.set_margin_end(10);
    all.set_child(Some(&base));

    all.set_height_request((1080.0 / 1.5) as i32);

    let output_box = gtk::Box::builder().build();
    output_box.append(&all);

    // let ui_scrollbar: gtk::Widget = all.vscrollbar();
    // let ui_scrollbar: gtk::Scrollbar = ui_scrollbar.dynamic_cast().expect("msg");

    let v2 = v.clone();

    let temp = { v2.borrow().scroll_value.clone() };

    let temp = temp.map(|v| {
        gtk::Adjustment::builder()
            .lower(v.lower)
            .page_increment(v.page_increment)
            .page_size(v.page_size)
            .step_increment(v.step_increment)
            .upper(v.upper)
            .value(v.value)
            .build()
    });

    let temp = temp.as_ref();

    all.set_vadjustment(temp);

    let first_call = Rc::new(RefCell::new(true));

    // Right before returning the UI, set a tick callback to keep track of where the scrollbar
    // is so that it doesn't change the planned render of the saved scrollbar
    //
    // This is really jank to track the scroll bar and then set it on redrawing the UI
    all.add_tick_callback(move |scrollbar, _frame_clock| {
        let adjustment = scrollbar.vadjustment();

        if *first_call.borrow() {
            let temp = v2.borrow().scroll_value.clone();

            let temp = temp.map(|v| {
                gtk::Adjustment::builder()
                    .lower(v.lower)
                    .page_increment(v.page_increment)
                    .page_size(v.page_size)
                    .step_increment(v.step_increment)
                    .upper(v.upper)
                    .value(v.value)
                    .build()
            });

            scrollbar.set_vadjustment(temp.as_ref());

            *first_call.borrow_mut() = false;
        } else {
            let temp = AdjCopy {
                value: adjustment.value(),
                lower: adjustment.lower(),
                upper: adjustment.upper(),
                step_increment: adjustment.step_increment(),
                page_increment: adjustment.page_increment(),
                page_size: adjustment.page_size(),
            };

            v2.borrow_mut().scroll_value = Some(temp);
        }

        ControlFlow::Continue
    });

    output_box
}
