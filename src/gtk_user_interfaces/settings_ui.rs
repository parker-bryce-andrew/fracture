use crate::{
    application_channel_creator::UiChannelSide,
    global_application_state::{AVAILABLE_PRESETS, FOUND_VERSION, VERSION},
    gpu_mirror_display::postprocessing_shaders::DEFAULT_POSTPROCESSOR,
    shaders::{
        SHADER_COLOR_GRADIENT, SHADER_FLIP_HORIZONTAL, SHADER_FLIP_VERTICAL, SHADER_INVERT_COLORS,
        SHADER_ROTATE_LEFT, SHADER_SHOW_ALL_INPUTS,
    },
    ui_state::*,
};
use gtk4::{
    self as gtk, Adjustment, ApplicationWindow, Button, TextBuffer, ToggleButton,
    gdk::{Display, RGBA},
    gio::ApplicationFlags,
    glib::{self, ControlFlow, GString},
    prelude::*,
};
use rand::{Rng, seq::IndexedRandom};
use std::{cell::RefCell, rc::Rc, sync::Mutex};
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

                *&mut state.borrow_mut().update_no_rebuild().green_screen =
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

                state.borrow_mut().update_no_rebuild().green_screen =
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

        state.borrow_mut().update_no_rebuild().frame_transparency = v;
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

                *&mut state.borrow_mut().update_no_rebuild().background = WindowBackground::Color(
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

        result.need_rebuild = false;

        *v2.borrow_mut().update_no_rebuild() = result;
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

                let text = serde_json::to_string_pretty(&temp).unwrap();

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

            let parse_result = serde_json::from_str::<SetUiState>(&after);

            if let Ok(parsed) = parse_result {
                import_dia.close();

                let new = parsed.build_new_full_settings_state();
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
