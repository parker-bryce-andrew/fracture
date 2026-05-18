use crate::gpu_mirror_display::state::Application;
use std::time::Duration;
use winit::event_loop::ActiveEventLoop;

pub fn start_shutdown(s: &mut Application) {
    s.app_state.intricate_todo_refactor.should_shutdown = true;
}

#[derive(Debug)]
pub struct ShutdownResult {
    gtk_settings_ui: Result<(), SettingsGtkShutdownErr>,
    pipewire_threads: Result<(), PipewireShutdownErr>,
}

#[derive(Debug)]
pub enum SettingsGtkShutdownErr {
    ThreadAlreadyTerminated,
    Disconnected(std::sync::mpsc::RecvTimeoutError),
    FailedWithinTimeLimit,
}

#[derive(Debug)]
pub enum PipewireShutdownErr {
    ThreadAlreadyTerminated,
    TimeoutOrTermination(std::sync::mpsc::RecvTimeoutError),
}

pub fn shutdown(ev: &ActiveEventLoop, app: Application) -> Result<ShutdownResult, ShutdownResult> {
    println!("Shutting down.");

    let pw_1 = app.external.channels.terminate_pipewire_stream.send(());
    let gtk_1 = app.external.channels.terminate_settings_ui.send(());

    let gtk;

    let mut count = 0;

    if gtk_1.is_ok() {
        'bad_logic_loop: loop {
            count += 1;

            // The termination check is completed when starting

            let r1 = app.external.settings_ui.gtk_open_signal(&app);
            let r2 = app.external.settings_ui.gtk_shutdown_signal(&app);

            // There's a case where the termination signal kills the thread before the channels are used. There
            // was an expect in the open and closing, so now I'm checking the errors reported to make sure
            // I didn't make any other mistakes that I didn't expect.
            if r1.is_err() || r2.is_err() {
                let result = (r1, r2);
                println!(
                    "The Setting UI is predicted to have shutdown already: {:#?}",
                    result
                );
            }

            match app
                .external
                .channels
                .ui_shutdown_conf
                .recv_timeout(Duration::from_millis(100))
            {
                Ok(_) => {
                    gtk = Ok(());
                    break 'bad_logic_loop;
                }
                Err(e) => match e {
                    std::sync::mpsc::RecvTimeoutError::Timeout => {
                        // 10 seconds
                        if count > 100 {
                            gtk = Err(SettingsGtkShutdownErr::FailedWithinTimeLimit);

                            break 'bad_logic_loop;
                        }
                    }
                    std::sync::mpsc::RecvTimeoutError::Disconnected => {
                        gtk = Err(SettingsGtkShutdownErr::Disconnected(
                            std::sync::mpsc::RecvTimeoutError::Disconnected,
                        ));
                        break 'bad_logic_loop;
                    }
                },
            }
        }
    } else {
        gtk = Err(SettingsGtkShutdownErr::ThreadAlreadyTerminated);
    }

    let pw;

    if pw_1.is_ok() {
        // For other Desktop Environments that are not Gnome, there's a loop that sleeps for 5 seconds. If unlucky,
        // it can require waiting a full 5 seconds.
        match app
            .external
            .channels
            .dbus_shutdown_conf
            .recv_timeout(Duration::from_millis(5500))
        {
            Ok(_) => pw = Ok(()),
            Err(e) => pw = Err(PipewireShutdownErr::TimeoutOrTermination(e)),
        }
    } else {
        pw = Err(PipewireShutdownErr::ThreadAlreadyTerminated);
    }

    let full = ShutdownResult {
        gtk_settings_ui: gtk,
        pipewire_threads: pw,
    };

    let wrapped;

    if full.gtk_settings_ui.is_ok() && full.pipewire_threads.is_ok() {
        wrapped = Ok(full);
    } else {
        wrapped = Err(full);
    }

    println!("{:#?}", wrapped);

    match wrapped {
        Ok(_) => {}
        Err(_) => {
            println!(
                "The process is exiting abnormally. There was an error reported on one of the threads and I think calling to exit the event loop will hang."
            );

            std::process::abort();
        }
    }

    println!("Attempting to drop application data");

    // I think the window handle needs to be dropped before calling exit.
    std::mem::drop(app);

    println!("Attempting to exit event loop.");
    // if either thread fails to shutdown i've not been successful calling `exit`. maybe
    // it's UB, maybe it's the foreign code, i don't know yet.
    ev.exit();

    wrapped
}
