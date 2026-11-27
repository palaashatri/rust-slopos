//! slopos-file-chooser: the dialog program behind the file chooser portal.
//!
//! - No arguments: read a `ChooserSpec` (JSON) on stdin, show the dialog, and
//!   print a `ChooserReply` (JSON) on stdout. Closing the window is a cancel.
//! - `--snapshot OUT.png [SPEC.json]`: render the dialog to a PNG without a
//!   display (for review and tests).
//! - `open-with FILE`: list the applications that open FILE's type; the chosen
//!   one is started on FILE once and the default is left alone (no
//!   default-application writer exists yet). Prints the outcome as JSON.
//! - `open-with --snapshot OUT.png FILE`: render the Open With dialog to a PNG.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use slopos_core::mime;
use slopos_portal::spec::{resolve_folder, ChooserReply, ChooserSpec, Kind};
use slopos_ui::accessibility::window_tree;
use slopos_ui::controls::file_chooser::{ChooserFilter, ChooserOutcome, FileChooser};
use slopos_ui::controls::open_with::{OpenWithChooser, OpenWithItem, OpenWithOutcome};
use slopos_ui::event::Event;
use slopos_ui::foundation::geometry::Scale;
use slopos_ui::platform::{self, App, Control, WindowOptions};
use slopos_ui::render::RasterSurface;

const WIDTH: u32 = 560;
const HEIGHT: u32 = 420;
const CHOOSER_ID: u64 = 100;
const OPEN_WITH_ID: u64 = 200;

fn main() -> ExitCode {
    if std::env::var_os("RUST_LOG").is_some() {
        let _ = env_logger::try_init();
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("open-with") => open_with(&args[1..]),
        Some("--snapshot") => snapshot_chooser(&args[1..]),
        Some(other) => {
            eprintln!("slopos-file-chooser: unknown argument {other:?}");
            ExitCode::FAILURE
        }
        None => run_chooser(),
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// The dialog for a request. The folder falls back to home when the request
/// names none or one that does not exist.
fn chooser_from_spec(spec: &ChooserSpec) -> FileChooser {
    let dir = resolve_folder(spec.folder.as_deref(), &home());
    let filters: Vec<ChooserFilter> = spec
        .filters
        .iter()
        .map(|filter| {
            let rules = filter.clone();
            ChooserFilter::new(&filter.name, move |name| rules.accepts(name))
        })
        .collect();
    let mut chooser = match spec.kind {
        Kind::Open => {
            FileChooser::open_with_filters(CHOOSER_ID, &spec.title, &dir, filters, spec.multiple)
        }
        Kind::OpenFolder => FileChooser::select_folder(CHOOSER_ID, &spec.title, &dir),
        Kind::Save => FileChooser::save(
            CHOOSER_ID,
            &spec.title,
            &dir,
            filters,
            spec.name.as_deref().unwrap_or(""),
        ),
    };
    if let Some(index) = spec.current_filter {
        chooser = chooser.with_active_filter(index);
    }
    if let Some(label) = &spec.accept_label {
        chooser = chooser.with_accept_label(label);
    }
    chooser.layout((WIDTH, HEIGHT));
    chooser
}

struct ChooserApp {
    chooser: FileChooser,
    window: (u32, u32),
    reply: Arc<Mutex<Option<ChooserReply>>>,
}

impl ChooserApp {
    fn finish(&self, reply: ChooserReply, control: &mut Control) {
        if let Ok(mut slot) = self.reply.lock() {
            *slot = Some(reply);
        }
        control.quit();
    }
}

impl App for ChooserApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => return self.finish(ChooserReply::Cancelled, control),
            Event::Resized { width, height } => {
                self.window = (*width, *height);
                self.chooser.layout(self.window);
            }
            Event::Wake => {}
            _ => match self.chooser.handle(event, control.modifiers()) {
                ChooserOutcome::Chosen(path) => {
                    return self.finish(ChooserReply::Chosen { paths: vec![path] }, control)
                }
                ChooserOutcome::ChosenMany(paths) => {
                    return self.finish(ChooserReply::Chosen { paths }, control)
                }
                ChooserOutcome::Cancelled => return self.finish(ChooserReply::Cancelled, control),
                ChooserOutcome::Copy(text) => control.set_clipboard(&text),
                ChooserOutcome::PasteRequested => control.request_paste(),
                ChooserOutcome::Ignored | ChooserOutcome::Changed => {}
            },
        }
        control.request_redraw();
    }

    fn paint(&mut self, surface: &RasterSurface) {
        if let Err(error) = self.chooser.paint(surface) {
            eprintln!("slopos-file-chooser: paint failed: {error}");
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        Some(window_tree(
            &self.chooser.title,
            self.chooser.accessibility_nodes(),
            Some(self.chooser.focus_node()),
        ))
    }
}

fn run_chooser() -> ExitCode {
    let mut input = String::new();
    if let Err(error) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("slopos-file-chooser: could not read the request: {error}");
        return ExitCode::FAILURE;
    }
    let spec: ChooserSpec = match serde_json::from_str(&input) {
        Ok(spec) => spec,
        Err(error) => {
            eprintln!("slopos-file-chooser: invalid request: {error}");
            return ExitCode::FAILURE;
        }
    };
    let reply = Arc::new(Mutex::new(None));
    let app = ChooserApp {
        chooser: chooser_from_spec(&spec),
        window: (WIDTH, HEIGHT),
        reply: reply.clone(),
    };
    let options = WindowOptions::new(&spec.title, "slopos-file-chooser", WIDTH, HEIGHT);
    if let Err(error) = platform::run(options, app, |_| {}) {
        eprintln!("slopos-file-chooser: {error}");
        return ExitCode::FAILURE;
    }
    let reply = reply
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .unwrap_or(ChooserReply::Cancelled);
    match serde_json::to_string(&reply) {
        Ok(json) => {
            let mut stdout = std::io::stdout();
            let _ = writeln!(stdout, "{json}");
            let _ = stdout.flush();
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("slopos-file-chooser: could not encode the reply: {error}");
            ExitCode::FAILURE
        }
    }
}

fn snapshot_chooser(args: &[String]) -> ExitCode {
    let Some(output) = args.first() else {
        eprintln!("usage: slopos-file-chooser --snapshot OUT.png [SPEC.json]");
        return ExitCode::FAILURE;
    };
    let spec = match args.get(1) {
        Some(path) => match std::fs::read_to_string(path).map(|text| serde_json::from_str(&text)) {
            Ok(Ok(spec)) => spec,
            _ => {
                eprintln!("slopos-file-chooser: cannot read the spec {path}");
                return ExitCode::FAILURE;
            }
        },
        None => ChooserSpec::new(Kind::Open, "Open File"),
    };
    let mut chooser = chooser_from_spec(&spec);
    write_snapshot(&mut |surface| chooser.paint(surface), output)
}

fn open_with(args: &[String]) -> ExitCode {
    let (snapshot, file) = match args {
        [flag, out, file] if flag == "--snapshot" => (Some(out.clone()), file.clone()),
        [file] => (None, file.clone()),
        _ => {
            eprintln!("usage: slopos-file-chooser open-with [--snapshot OUT.png] FILE");
            return ExitCode::FAILURE;
        }
    };
    let path = PathBuf::from(&file);
    let associations = mime::applications_for(&mime::guess(&path));
    let items: Vec<OpenWithItem> = associations
        .iter()
        .map(|association| OpenWithItem {
            label: association.app.name.clone(),
            is_default: association.is_default,
        })
        .collect();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.clone());
    let mut chooser = OpenWithChooser::new(OPEN_WITH_ID, "Open With", &file_name, items);
    chooser.layout((WIDTH, HEIGHT));
    if let Some(output) = snapshot {
        return write_snapshot(&mut |surface| chooser.paint(surface), &output);
    }
    let outcome: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let app = OpenWithApp {
        chooser,
        associations: associations.clone(),
        path: path.clone(),
        outcome: outcome.clone(),
    };
    let options = WindowOptions::new("Open With", "slopos-file-chooser", WIDTH, HEIGHT);
    if let Err(error) = platform::run(options, app, |_| {}) {
        eprintln!("slopos-file-chooser: {error}");
        return ExitCode::FAILURE;
    }
    let result = outcome.lock().ok().and_then(|mut slot| slot.take());
    match result {
        Some(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        None => {
            println!(r#"{{"status":"cancelled"}}"#);
            ExitCode::SUCCESS
        }
    }
}

struct OpenWithApp {
    chooser: OpenWithChooser,
    associations: Vec<mime::Association>,
    path: PathBuf,
    outcome: Arc<Mutex<Option<String>>>,
}

impl OpenWithApp {
    fn choose(&self, index: usize, control: &mut Control) {
        let Some(association) = self.associations.get(index) else {
            return;
        };
        let started = mime::launch_with(&association.app, &self.path);
        let json = serde_json::json!({
            "status": if started.is_some() { "opened" } else { "failed" },
            "app_id": association.app.id,
            "default_changed": false,
        });
        if let Ok(mut slot) = self.outcome.lock() {
            *slot = Some(json.to_string());
        }
        control.quit();
    }
}

impl App for OpenWithApp {
    fn event(&mut self, event: &Event, control: &mut Control) {
        match event {
            Event::CloseRequested => control.quit(),
            Event::Resized { width, height } => self.chooser.layout((*width, *height)),
            Event::Wake => {}
            _ => match self.chooser.handle(event, control.modifiers()) {
                OpenWithOutcome::Chosen(index) => return self.choose(index, control),
                OpenWithOutcome::Cancelled => control.quit(),
                OpenWithOutcome::Changed | OpenWithOutcome::Ignored => {}
            },
        }
        control.request_redraw();
    }

    fn paint(&mut self, surface: &RasterSurface) {
        if let Err(error) = self.chooser.paint(surface) {
            eprintln!("slopos-file-chooser: paint failed: {error}");
        }
    }

    fn accessibility(&self) -> Option<accesskit::TreeUpdate> {
        Some(window_tree(
            &self.chooser.title,
            self.chooser.accessibility_nodes(),
            Some(self.chooser.focus_node()),
        ))
    }
}

fn write_snapshot(
    paint: &mut dyn FnMut(&RasterSurface) -> Result<(), slopos_ui::render::RenderError>,
    output: &str,
) -> ExitCode {
    let scale = Scale::new(1).expect("scale 1 is valid");
    let surface = match RasterSurface::new(WIDTH, HEIGHT, scale) {
        Ok(surface) => surface,
        Err(error) => {
            eprintln!("slopos-file-chooser: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = paint(&surface) {
        eprintln!("slopos-file-chooser: paint failed: {error}");
        return ExitCode::FAILURE;
    }
    match surface.save_png(Path::new(output)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("slopos-file-chooser: {error}");
            ExitCode::FAILURE
        }
    }
}
