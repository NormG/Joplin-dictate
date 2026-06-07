use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, Calendar, CheckButton, ComboBoxText,
    CssProvider, Entry, Grid, Label, Orientation, Popover, STYLE_PROVIDER_PRIORITY_APPLICATION,
    SpinButton, gdk,
};
use joplin_dictate::{Config, CreateOptions, Folder, list_folders, run_workflow};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use std::cell::RefCell;
use std::path::PathBuf;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

const APP_ID: &str = "dev.normg.joplin-dictate";

#[derive(Clone)]
struct Ui {
    notebook: ComboBoxText,
    title: Entry,
    todo: CheckButton,
    due_button: Button,
    calendar: Calendar,
    calendar_popover: Popover,
    set_due: Button,
    clear_due: Button,
    hour: SpinButton,
    minute: SpinButton,
    record: Button,
    status: Label,
    due_date: Rc<RefCell<Option<(i32, u32, u32)>>>,
    recording: Rc<RefCell<Option<RecordingState>>>,
}

fn install_recording_button_css() {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let provider = CssProvider::new();
    provider.load_from_data(
        "button.ready-record {
            background: #1c6ea4;
            color: #ffffff;
            border-color: #155985;
        }
        button.ready-record:hover {
            background: #2685c7;
            color: #ffffff;
        }
        button.ready-record:disabled {
            background: #315f7d;
            color: #ffffff;
        }
        button.recording {
            background: #c01c28;
            color: #ffffff;
            border-color: #a51d2d;
        }
        button.recording:hover {
            background: #e01b24;
            color: #ffffff;
        }
        button.recording:disabled {
            background: #8b1a24;
            color: #ffffff;
        }",
    );
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

struct RecordingState {
    child: Child,
    temp: TempDir,
    wav: PathBuf,
    options: CreateOptions,
}

fn main() {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    install_recording_button_css();
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Joplin Dictate")
        .default_width(460)
        .resizable(false)
        .build();

    let outer = GtkBox::new(Orientation::Vertical, 10);
    outer.set_margin_top(14);
    outer.set_margin_bottom(14);
    outer.set_margin_start(14);
    outer.set_margin_end(14);
    window.set_child(Some(&outer));

    let grid = Grid::builder().column_spacing(8).row_spacing(8).build();
    outer.append(&grid);

    attach_label(&grid, "Notebook:", 0);
    let notebook = ComboBoxText::new();
    notebook.append(Some(""), "— default notebook —");
    notebook.set_active_id(Some(""));
    notebook.set_hexpand(true);
    grid.attach(&notebook, 1, 0, 1, 1);

    attach_label(&grid, "Title:", 1);
    let title = Entry::new();
    title.set_placeholder_text(Some("Auto (first line of transcript)"));
    title.set_hexpand(true);
    grid.attach(&title, 1, 1, 1, 1);

    let todo = CheckButton::with_label("Create as to-do");
    grid.attach(&todo, 1, 2, 1, 1);

    attach_label(&grid, "Due date:", 3);
    let due_row = GtkBox::new(Orientation::Horizontal, 4);
    let due_button = Button::with_label("— no due date —");
    due_button.set_hexpand(true);
    due_button.set_sensitive(false);
    due_row.append(&due_button);

    let hour = SpinButton::with_range(0.0, 23.0, 1.0);
    hour.set_value(9.0);
    hour.set_width_chars(2);
    hour.set_sensitive(false);
    due_row.append(&hour);
    due_row.append(&Label::new(Some(":")));

    let minute = SpinButton::with_range(0.0, 59.0, 5.0);
    minute.set_value(0.0);
    minute.set_width_chars(2);
    minute.set_sensitive(false);
    due_row.append(&minute);
    grid.attach(&due_row, 1, 3, 1, 1);

    let calendar = Calendar::new();
    let popover_box = GtkBox::new(Orientation::Vertical, 6);
    popover_box.set_margin_top(8);
    popover_box.set_margin_bottom(8);
    popover_box.set_margin_start(8);
    popover_box.set_margin_end(8);
    popover_box.append(&calendar);

    let pop_btns = GtkBox::new(Orientation::Horizontal, 4);
    let clear_due = Button::with_label("Clear");
    let set_due = Button::with_label("Set date");
    pop_btns.append(&clear_due);
    pop_btns.append(&set_due);
    popover_box.append(&pop_btns);

    let calendar_popover = Popover::new();
    calendar_popover.set_child(Some(&popover_box));
    calendar_popover.set_parent(&due_button);

    let record = Button::with_label("▶  Start Recording");
    record.add_css_class("ready-record");
    record.add_css_class("suggested-action");
    record.set_height_request(52);
    record.set_sensitive(false);
    outer.append(&record);

    let status = Label::new(Some(""));
    status.set_xalign(0.0);
    status.set_selectable(true);
    status.set_wrap(true);
    outer.append(&status);

    let ui = Ui {
        notebook,
        title,
        todo,
        due_button,
        calendar,
        calendar_popover,
        set_due,
        clear_due,
        hour,
        minute,
        record,
        status,
        due_date: Rc::new(RefCell::new(None)),
        recording: Rc::new(RefCell::new(None)),
    };

    wire_events(&ui);
    run_startup_checks(ui);
    window.present();
}

fn attach_label(grid: &Grid, text: &str, row: i32) {
    let label = Label::new(Some(text));
    label.set_xalign(1.0);
    grid.attach(&label, 0, row, 1, 1);
}

fn wire_events(ui: &Ui) {
    {
        let ui = ui.clone();
        let widget = ui.todo.clone();
        widget.connect_toggled(move |todo| {
            let active = todo.is_active();
            ui.due_button.set_sensitive(active);
            if !active {
                clear_due(&ui);
            }
        });
    }

    {
        let ui = ui.clone();
        ui.due_button
            .connect_clicked(move |_| ui.calendar_popover.popup());
    }

    {
        let ui = ui.clone();
        ui.set_due.connect_clicked(move |_| {
            let date = ui.calendar.date();
            let year = date.year();
            let month = date.month() as u32;
            let day = date.day_of_month() as u32;
            *ui.due_date.borrow_mut() = Some((year, month, day));
            ui.due_button
                .set_label(&format!("{day} {}", month_name(month, year)));
            ui.hour.set_sensitive(true);
            ui.minute.set_sensitive(true);
            ui.calendar_popover.popdown();
        });
    }

    {
        let ui = ui.clone();
        let widget = ui.clear_due.clone();
        widget.connect_clicked(move |_| clear_due(&ui));
    }

    {
        let ui = ui.clone();
        let widget = ui.record.clone();
        widget.connect_clicked(move |_| {
            if ui.recording.borrow().is_some() {
                stop_recording(ui.clone());
            } else {
                start_recording(ui.clone());
            }
        });
    }
}

fn month_name(month: u32, year: i32) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let idx = month.saturating_sub(1).min(11) as usize;
    format!("{} {}", MONTHS[idx], year)
}

fn clear_due(ui: &Ui) {
    *ui.due_date.borrow_mut() = None;
    ui.due_button.set_label("— no due date —");
    ui.hour.set_sensitive(false);
    ui.minute.set_sensitive(false);
    ui.calendar_popover.popdown();
}

fn run_startup_checks(ui: Ui) {
    ui.status.set_text("Checking environment…");
    let (sender, receiver) = mpsc::channel::<Result<Vec<Folder>, String>>();
    thread::spawn(move || {
        let result = (|| {
            let config = Config::load().map_err(|e| e.to_string())?;
            config.require_ready().map_err(|e| e.to_string())?;
            list_folders(&config).map_err(|e| e.to_string())
        })();
        let _ = sender.send(result);
    });

    gtk4::glib::timeout_add_local(Duration::from_millis(100), move || {
        match receiver.try_recv() {
            Ok(result) => {
                match result {
                    Ok(folders) => {
                        for f in folders {
                            ui.notebook.append(Some(&f.id), &f.title);
                        }
                        ui.status.set_text("Ready.");
                        ui.record.set_sensitive(true);
                    }
                    Err(e) => {
                        ui.status.set_text(&format!("⚠  {e}"));
                        ui.record.set_sensitive(false);
                    }
                }
                gtk4::glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => gtk4::glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => gtk4::glib::ControlFlow::Break,
        }
    });
}

fn start_recording(ui: Ui) {
    ui.record.set_sensitive(false);
    ui.status.set_text("Recording… press Stop when done.");

    let parent_id = ui
        .notebook
        .active_id()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let title_text = ui.title.text().to_string();
    let title = if title_text.trim().is_empty() {
        None
    } else {
        Some(title_text)
    };
    let is_todo = ui.todo.is_active();
    let due = ui.due_date.borrow().map(|(year, month, day)| {
        format!(
            "{year:04}-{month:02}-{day:02} {:02}:{:02}",
            ui.hour.value_as_int(),
            ui.minute.value_as_int()
        )
    });
    let options = CreateOptions {
        parent_id,
        title,
        is_todo: is_todo || due.is_some(),
        due,
        audio_file: None,
    };

    match start_arecord(options) {
        Ok(state) => {
            *ui.recording.borrow_mut() = Some(state);
            ui.record.set_label("⏹  Stop Recording");
            ui.record.remove_css_class("suggested-action");
            ui.record.remove_css_class("ready-record");
            ui.record.add_css_class("destructive-action");
            ui.record.add_css_class("recording");
            ui.record.set_sensitive(true);
        }
        Err(e) => {
            ui.status.set_text(&format!("Error: {e}"));
            ui.record.set_sensitive(true);
        }
    }
}

fn start_arecord(options: CreateOptions) -> Result<RecordingState, String> {
    let temp = TempDir::new().map_err(|e| format!("Failed to create temp dir: {e}"))?;
    let wav = temp.path().join("recording.wav");
    // Pipe stderr so we can surface arecord device errors to the status bar.
    // -q is intentionally omitted here so errors are not silenced.
    let child = Command::new("pw-record")
        .args(["--format=s16", "--rate=16000", "--channels=1"])
        .arg(&wav)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start pw-record: {e}"))?;
    Ok(RecordingState {
        child,
        temp,
        wav,
        options,
    })
}

fn stop_recording(ui: Ui) {
    let Some(mut state) = ui.recording.borrow_mut().take() else {
        return;
    };

    ui.record.set_sensitive(false);
    ui.status.set_text("Transcribing and creating note…");
    ui.record.set_label("▶  Start Recording");
    ui.record.remove_css_class("destructive-action");
    ui.record.remove_css_class("recording");
    ui.record.add_css_class("ready-record");
    ui.record.add_css_class("suggested-action");

    let pid = Pid::from_raw(state.child.id() as i32);
    let _ = kill(pid, Signal::SIGINT);

    let (sender, receiver) = mpsc::channel::<Result<String, String>>();
    thread::spawn(move || {
        let result = (|| {
            // Grab the stderr handle before wait() consumes the child.
            let mut stderr_handle = state.child.stderr.take();
            let _ = state.child.wait();
            let _keep_temp_alive = state.temp;

            // If the WAV is missing or empty, surface the arecord error message.
            let wav_size = std::fs::metadata(&state.wav)
                .map(|m| m.len())
                .unwrap_or(0);
            if wav_size == 0 {
                let mut arecord_err = String::new();
                if let Some(ref mut h) = stderr_handle {
                    let _ = h.read_to_string(&mut arecord_err);
                }
                let arecord_err = arecord_err.trim().to_string();
                return Err(if arecord_err.is_empty() {
                    "No audio captured — check microphone is connected and not muted"
                        .to_string()
                } else {
                    format!("arecord error: {arecord_err}")
                });
            }

            let config = Config::load().map_err(|e| e.to_string())?;
            state.options.audio_file = Some(state.wav);
            let created = run_workflow(&config, &state.options).map_err(|e| e.to_string())?;
            Ok(match created {
                Some(note) => {
                    let kind = if note.is_todo { "to-do" } else { "note" };
                    let mut msg = format!(
                        "Created Joplin {kind}: {}  |  Title: {}",
                        note.id, note.title
                    );
                    if let Some(due) = note.due_human {
                        msg.push_str(&format!("  |  Due: {due}"));
                    }
                    msg
                }
                None => "No speech detected.".to_string(),
            })
        })();
        let _ = sender.send(result);
    });

    gtk4::glib::timeout_add_local(Duration::from_millis(100), move || {
        match receiver.try_recv() {
            Ok(result) => {
                match result {
                    Ok(msg) => ui.status.set_text(&msg),
                    Err(e) => ui.status.set_text(&format!("Error: {e}")),
                }
                ui.record.set_sensitive(true);
                gtk4::glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => gtk4::glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => gtk4::glib::ControlFlow::Break,
        }
    });
}
