//! Headless tests of the window: fake sources, injected events and the
//! accessibility tree. Nothing here reads `/proc` or sends a real signal.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::time::Instant;

use accesskit::NodeId;
use slopos_ui::event::{AccessibilityRequest, Event, Key, Modifiers, PointerButton};
use slopos_ui::foundation::geometry::Scale;
use slopos_ui::platform::{App, Control};
use slopos_ui::render::RasterSurface;

use super::*;
use crate::gpu::GpuInfo;
use crate::model::{FsSpace, ProcSample, Snapshot};
use crate::policy::Identity;
use crate::procfs::{CpuTicks, DiskStat, MemInfo};

const USER: u32 = 1000;

struct FakeSource {
    snapshot: Snapshot,
    identities: HashMap<u32, Identity>,
    sent: RefCell<Vec<u32>>,
    send_error: Option<io::ErrorKind>,
    gpu: GpuInfo,
}

impl FakeSource {
    fn new(snapshot: Snapshot) -> Self {
        Self {
            snapshot,
            identities: HashMap::new(),
            sent: RefCell::new(Vec::new()),
            send_error: None,
            gpu: GpuInfo::Unavailable("no adapter in the test".into()),
        }
    }
}

impl Source for FakeSource {
    fn sample(&mut self, at: Instant) -> Snapshot {
        Snapshot {
            at,
            ..self.snapshot.clone()
        }
    }
    fn identity(&self, pid: u32) -> Option<Identity> {
        self.identities.get(&pid).copied()
    }
    fn send_sigterm(&self, pid: u32) -> io::Result<()> {
        self.sent.borrow_mut().push(pid);
        match self.send_error {
            Some(kind) => Err(io::Error::from(kind)),
            None => Ok(()),
        }
    }
    fn current_user(&self) -> u32 {
        USER
    }
    fn user_names(&self) -> HashMap<u32, String> {
        HashMap::from([(USER, "user".to_owned()), (0, "root".to_owned())])
    }
    fn gpu(&mut self) -> GpuInfo {
        self.gpu.clone()
    }
}

fn process(pid: u32, name: &str, uid: u32, start: u64, ticks: u64) -> ProcSample {
    ProcSample {
        pid,
        ppid: 1,
        name: name.to_owned(),
        state: 'S',
        uid: Some(uid),
        cpu_ticks: ticks,
        start_ticks: start,
        threads: Some(2),
        rss_bytes: Some(3 * 1024 * 1024),
        cmdline: format!("/usr/bin/{name} --flag"),
    }
}

fn snapshot(total: u64, idle: u64, processes: Vec<ProcSample>) -> Snapshot {
    Snapshot {
        at: Instant::now(),
        cpu: Some((
            CpuTicks { total, idle },
            vec![
                CpuTicks {
                    total: total / 2,
                    idle: idle / 2,
                },
                CpuTicks {
                    total: total / 2,
                    idle: idle / 2,
                },
            ],
        )),
        memory: Some(MemInfo {
            total: Some(2 * 1024 * 1024 * 1024),
            free: Some(512 * 1024 * 1024),
            available: Some(1024 * 1024 * 1024),
            buffers: Some(0),
            cached: Some(256 * 1024 * 1024),
        }),
        disks: Some(vec![DiskStat {
            name: "sda".into(),
            sectors_read: 0,
            sectors_written: 0,
        }]),
        filesystem: Some(FsSpace {
            total: 100 * 1024 * 1024 * 1024,
            free: 40 * 1024 * 1024 * 1024,
        }),
        processes: Some(processes),
    }
}

fn base_processes() -> Vec<ProcSample> {
    vec![
        process(1, "init", 0, 1, 0),
        process(4000, "runaway", USER, 77, 0),
        process(4001, "editor", USER, 80, 0),
        process(5000, "rootd", 0, 90, 0),
    ]
}

fn app_with(source: FakeSource) -> SystemMonitor<FakeSource> {
    SystemMonitor::new(source, 760, 540)
}

/// Show the window and take the first sample.
fn show(app: &mut SystemMonitor<FakeSource>) -> Control {
    let mut control = Control::default();
    app.event(&Event::Visibility(true), &mut control);
    control
}

/// Pass the sample timer, then a second sample with CPU time elapsed.
fn second_sample(app: &mut SystemMonitor<FakeSource>, processes: Vec<ProcSample>) {
    app.source.snapshot = snapshot(1200, 550, processes);
    let mut control = Control::default();
    app.event(&Event::Timer(SAMPLE_TIMER), &mut control);
}

/// A later sample, with more CPU time elapsed than the previous one.
fn later_sample(app: &mut SystemMonitor<FakeSource>, total: u64, processes: Vec<ProcSample>) {
    app.source.snapshot = snapshot(total, total / 2, processes);
    let mut control = Control::default();
    app.event(&Event::Timer(SAMPLE_TIMER), &mut control);
}

fn click(app: &mut SystemMonitor<FakeSource>, x: i32, y: i32) {
    let mut control = Control::default();
    app.event(
        &Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
        },
        &mut control,
    );
    app.event(
        &Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
        },
        &mut control,
    );
}

fn center(rect: Rect) -> (i32, i32) {
    (
        rect.x + rect.width as i32 / 2,
        rect.y + rect.height as i32 / 2,
    )
}

fn select_pid(app: &mut SystemMonitor<FakeSource>, pid: u32) {
    let row = app
        .visible
        .iter()
        .position(|&index| app.view().processes[index].pid == pid)
        .expect("the process is listed");
    let rect = app.list.row_rect(row);
    let (x, y) = center(rect);
    click(app, x, y);
    assert_eq!(app.selected_pid, Some(pid));
}

fn label_texts(app: &SystemMonitor<FakeSource>) -> Vec<String> {
    app.labels().into_iter().map(|label| label.text).collect()
}

fn open_end_dialog(app: &mut SystemMonitor<FakeSource>) {
    let (x, y) = center(app.end.bounds);
    click(app, x, y);
    assert!(app.dialog.is_some(), "End process opens the confirmation");
}

fn choose(app: &mut SystemMonitor<FakeSource>, node: u64) {
    let mut control = Control::default();
    app.event(
        &Event::Accessibility(AccessibilityRequest::Click(node)),
        &mut control,
    );
}

#[test]
fn sampling_runs_only_while_the_window_is_visible() {
    let mut app = app_with(FakeSource::new(snapshot(1000, 500, base_processes())));
    let mut control = Control::default();
    app.event(&Event::Visibility(false), &mut control);
    assert!(
        control.timer_requests().is_empty(),
        "hidden windows do not sample"
    );

    let mut control = show(&mut app);
    assert_eq!(control.timer_requests(), &[(SAMPLE_PERIOD, SAMPLE_TIMER)]);

    // A second show while the timer is pending does not start another.
    let mut again = Control::default();
    app.event(&Event::Visibility(true), &mut again);
    assert!(again.timer_requests().is_empty());

    // Each tick samples and re-arms while visible.
    let mut tick = Control::default();
    app.event(&Event::Timer(SAMPLE_TIMER), &mut tick);
    assert_eq!(tick.timer_requests(), &[(SAMPLE_PERIOD, SAMPLE_TIMER)]);

    // Hidden: the pending tick does nothing and stops the chain.
    let mut hide = Control::default();
    app.event(&Event::Visibility(false), &mut hide);
    let mut stopped = Control::default();
    app.event(&Event::Timer(SAMPLE_TIMER), &mut stopped);
    assert!(stopped.timer_requests().is_empty());
    control.request_redraw();
}

#[test]
fn cpu_share_and_process_sort_come_from_two_samples() {
    let mut app = app_with(FakeSource::new(snapshot(1000, 500, base_processes())));
    show(&mut app);
    assert!(label_texts(&app)
        .iter()
        .all(|text| !text.contains("%") || text.contains("Showing")));
    second_sample(
        &mut app,
        vec![
            process(1, "init", 0, 1, 0),
            process(4000, "runaway", USER, 77, 100),
            process(4001, "editor", USER, 80, 0),
            process(5000, "rootd", 0, 90, 0),
        ],
    );
    // 700 of 700 total ticks... the snapshot grew by 200 total and 50 idle: 75% busy.
    app.tabs.select(CPU);
    let texts = label_texts(&app);
    assert!(
        texts.contains(&"Total usage: 75.0%".to_owned()),
        "{texts:?}"
    );
    assert!(texts.contains(&"Logical CPUs: 2".to_owned()));
    // The runaway process used 100 of 200 ticks.
    assert_eq!(app.view().processes[0].pid, 4000);
    assert_eq!(app.view().processes[0].cpu_percent, Some(50.0));
}

#[test]
fn memory_and_disk_views_show_only_read_values() {
    let mut source = FakeSource::new(snapshot(1000, 500, base_processes()));
    source.snapshot.memory = Some(MemInfo {
        total: Some(2 * 1024 * 1024 * 1024),
        available: None,
        ..MemInfo::default()
    });
    let mut app = app_with(source);
    show(&mut app);
    app.tabs.select(MEMORY);
    let texts = label_texts(&app);
    assert!(texts.contains(&"Total: 2.0 GiB".to_owned()), "{texts:?}");
    assert!(
        texts.contains(&"Used: unavailable".to_owned()),
        "no MemAvailable means no used figure"
    );

    app.tabs.select(DISK);
    let texts = label_texts(&app);
    assert!(
        texts
            .iter()
            .any(|text| text == "Root file system (/): 40.0 GiB available of 100.0 GiB"),
        "{texts:?}"
    );
    assert!(texts.contains(&"Read: measuring".to_owned()));
}

#[test]
fn gpu_is_unavailable_unless_the_probe_finds_an_adapter() {
    let mut app = app_with(FakeSource::new(snapshot(1000, 500, base_processes())));
    show(&mut app);
    choose_tab(&mut app, GPU);
    let texts = label_texts(&app);
    assert!(texts.contains(&"Adapter: unavailable".to_owned()));
    assert!(texts
        .iter()
        .any(|text| text.starts_with("Reason: no adapter")));

    let mut source = FakeSource::new(snapshot(1000, 500, base_processes()));
    source.gpu = GpuInfo::Adapter {
        name: "Test GPU".into(),
        backend: "Vulkan".into(),
    };
    let mut app = app_with(source);
    show(&mut app);
    choose_tab(&mut app, GPU);
    let texts = label_texts(&app);
    assert!(texts.contains(&"Adapter: Test GPU".to_owned()));
    assert!(texts.contains(&"Backend: Vulkan".to_owned()));
}

fn choose_tab(app: &mut SystemMonitor<FakeSource>, index: usize) {
    let rect = app.tabs.tab_rect(index).expect("the tab exists");
    let (x, y) = center(rect);
    click(app, x, y);
    assert_eq!(app.view_index(), index);
}

#[test]
fn the_filter_narrows_the_list_and_keeps_the_selection_on_the_same_process() {
    let mut app = app_with(FakeSource::new(snapshot(1000, 500, base_processes())));
    show(&mut app);
    assert_eq!(app.visible.len(), 4);
    select_pid(&mut app, 4001);

    // Type "run" into the filter, which has keyboard focus.
    let mut control = Control::default();
    app.focus_on(Focus::Filter);
    for ch in "run".chars() {
        app.event(
            &Event::KeyPressed {
                key: Key::Character(ch),
                modifiers: Modifiers::default(),
            },
            &mut control,
        );
    }
    assert_eq!(app.visible.len(), 1);
    assert_eq!(
        app.selected_pid, None,
        "the selected process is filtered out"
    );

    // Clearing the filter brings it back and selects nothing until chosen.
    app.filter.set_text("");
    app.refresh_rows();
    assert_eq!(app.visible.len(), 4);
    assert!(!app.end.enabled);
}

#[test]
fn a_selection_survives_a_resort_without_scrolling() {
    let mut app = app_with(FakeSource::new(snapshot(1000, 500, base_processes())));
    show(&mut app);
    second_sample(
        &mut app,
        vec![
            process(4000, "runaway", USER, 77, 0),
            process(4001, "editor", USER, 80, 100),
        ],
    );
    select_pid(&mut app, 4001);
    // Editor now uses more CPU and moves to the top; the CPU total grew too.
    later_sample(
        &mut app,
        1400,
        vec![
            process(4000, "runaway", USER, 77, 1),
            process(4001, "editor", USER, 80, 300),
        ],
    );
    assert_eq!(app.list.selection(), vec![0]);
    assert_eq!(app.selected_pid, Some(4001));
}

#[test]
fn end_process_refuses_init_and_other_users_without_asking() {
    let mut source = FakeSource::new(snapshot(1000, 500, base_processes()));
    source.identities.insert(
        1,
        Identity {
            start_ticks: 1,
            uid: Some(0),
            state: 'S',
        },
    );
    let mut app = app_with(source);
    show(&mut app);
    select_pid(&mut app, 1);
    // init is listed but End is refused before any dialog opens.
    let (x, y) = center(app.end.bounds);
    click(&mut app, x, y);
    assert!(app.dialog.is_none());
    assert_eq!(
        app.status(),
        "Refused: PID 1 is the init process and is never ended here."
    );

    select_pid(&mut app, 5000);
    click(&mut app, x, y);
    assert!(app.dialog.is_none());
    assert!(app
        .status()
        .starts_with("Refused: the process belongs to user 0"));
    assert!(app.source.sent.borrow().is_empty());
}

#[test]
fn no_is_the_default_and_nothing_is_sent_unless_yes_is_chosen() {
    let mut source = FakeSource::new(snapshot(1000, 500, base_processes()));
    source.identities.insert(
        4000,
        Identity {
            start_ticks: 77,
            uid: Some(USER),
            state: 'S',
        },
    );
    let mut app = app_with(source);
    show(&mut app);
    select_pid(&mut app, 4000);
    open_end_dialog(&mut app);

    // Enter chooses the default, which is No.
    let mut control = Control::default();
    app.event(
        &Event::KeyPressed {
            key: Key::Enter,
            modifiers: Modifiers::default(),
        },
        &mut control,
    );
    assert!(app.dialog.is_none());
    assert!(app.source.sent.borrow().is_empty());
    assert_eq!(app.status(), "End process cancelled.");

    open_end_dialog(&mut app);
    choose(&mut app, DIALOG_ID + 11); // Yes
    assert_eq!(*app.source.sent.borrow(), vec![4000]);
    assert!(app
        .status()
        .starts_with("Sent SIGTERM to \"runaway\" (PID 4000)"));
    // The read-back timer is started once the signal is sent.
    let mut control = Control::default();
    app.source.identities.remove(&4000);
    app.event(&Event::Timer(READBACK_TIMER), &mut control);
    assert_eq!(app.status(), "PID 4000 (\"runaway\") has ended.");
}

#[test]
fn a_reused_pid_at_confirmation_time_is_not_signalled() {
    let mut source = FakeSource::new(snapshot(1000, 500, base_processes()));
    source.identities.insert(
        4000,
        Identity {
            start_ticks: 77,
            uid: Some(USER),
            state: 'S',
        },
    );
    let mut app = app_with(source);
    show(&mut app);
    select_pid(&mut app, 4000);
    open_end_dialog(&mut app);
    // Another process took the PID while the dialog was open.
    app.source.identities.insert(
        4000,
        Identity {
            start_ticks: 999,
            uid: Some(USER),
            state: 'S',
        },
    );
    choose(&mut app, DIALOG_ID + 11); // Yes
    assert!(app.source.sent.borrow().is_empty());
    assert!(app
        .status()
        .starts_with("Not ended: the PID now belongs to another process"));
}

#[test]
fn a_still_running_process_after_sigterm_is_reported_as_such() {
    let mut source = FakeSource::new(snapshot(1000, 500, base_processes()));
    source.identities.insert(
        4000,
        Identity {
            start_ticks: 77,
            uid: Some(USER),
            state: 'S',
        },
    );
    let mut app = app_with(source);
    show(&mut app);
    select_pid(&mut app, 4000);
    open_end_dialog(&mut app);
    choose(&mut app, DIALOG_ID + 11);
    let mut control = Control::default();
    app.event(&Event::Timer(READBACK_TIMER), &mut control);
    assert_eq!(app.status(), "PID 4000 is still running after SIGTERM.");
}

#[test]
fn a_failed_signal_is_reported_not_claimed() {
    let mut source = FakeSource::new(snapshot(1000, 500, base_processes()));
    source.identities.insert(
        4000,
        Identity {
            start_ticks: 77,
            uid: Some(USER),
            state: 'S',
        },
    );
    source.send_error = Some(io::ErrorKind::PermissionDenied);
    let mut app = app_with(source);
    show(&mut app);
    select_pid(&mut app, 4000);
    open_end_dialog(&mut app);
    choose(&mut app, DIALOG_ID + 11);
    assert!(
        app.status()
            .starts_with("Could not send SIGTERM to PID 4000"),
        "{}",
        app.status()
    );
    assert!(app.pending_readback.is_none());
}

#[test]
fn inspect_shows_the_command_line_and_reports_missing_values() {
    let mut app = app_with(FakeSource::new(snapshot(1000, 500, base_processes())));
    show(&mut app);
    select_pid(&mut app, 4000);
    let (x, y) = center(app.inspect.bounds);
    click(&mut app, x, y);
    let texts = label_texts(&app);
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with("Name: runaway    PID: 4000")),
        "{texts:?}"
    );
    assert!(texts.iter().any(|t| t.contains("/usr/bin/runaway --flag")));
    assert!(texts.iter().any(|t| t.starts_with("CPU: measuring")));
}

#[test]
fn accessibility_tree_exposes_tabs_labels_and_the_list() {
    let mut app = app_with(FakeSource::new(snapshot(1000, 500, base_processes())));
    show(&mut app);
    let tree = app.accessibility().expect("the window has a tree");
    let find = |id: u64| {
        tree.nodes
            .iter()
            .find(|(node, _)| *node == NodeId(id))
            .map(|(_, n)| n)
    };
    assert_eq!(
        find(TABS_ID).expect("tab list").role(),
        accesskit::Role::TabList
    );
    assert_eq!(
        find(FILTER_ID).expect("filter").role(),
        accesskit::Role::TextInput
    );
    assert!(find(LIST_ID).is_some());
    assert!(tree
        .nodes
        .iter()
        .any(|(_, node)| node.label() == Some("Processes")));

    choose_tab(&mut app, MEMORY);
    let tree = app.accessibility().expect("the window has a tree");
    assert!(tree
        .nodes
        .iter()
        .any(|(_, node)| node.label() == Some("Total: 2.0 GiB")));
    assert!(
        tree.nodes.iter().all(|(id, _)| *id != NodeId(FILTER_ID)),
        "no filter outside Processes"
    );
}

#[test]
fn accessibility_click_on_a_tab_selects_it() {
    let mut app = app_with(FakeSource::new(snapshot(1000, 500, base_processes())));
    show(&mut app);
    let node = app.tabs.tab_node_id(DISK);
    choose(&mut app, node);
    assert_eq!(app.view_index(), DISK);
}

#[test]
fn every_view_paints_without_error() {
    let mut app = app_with(FakeSource::new(snapshot(1000, 500, base_processes())));
    show(&mut app);
    second_sample(&mut app, base_processes());
    let scale = Scale::new(1).expect("1x");
    let surface = RasterSurface::new(760, 540, scale).expect("a surface");
    for index in 0..TABS.len() {
        choose_tab(&mut app, index);
        app.paint(&surface);
    }
    // The painted window has pixels in it.
    assert!(surface.pixel(10, 10).is_some());
}

#[test]
fn process_columns_fill_the_list_width() {
    let columns = process_columns(700);
    let total: u32 = columns.iter().map(|(_, width)| width).sum();
    assert_eq!(total, 700);
    assert!(columns[0].1 >= 80);
    assert_eq!(
        process_columns(10)[0].1,
        80,
        "the name column keeps a minimum width"
    );
}

#[test]
fn long_text_is_shortened_with_an_ellipsis() {
    let font = Font::system(12.0);
    let shortened = fit("a very long process name that will not fit", &font, 60);
    assert!(shortened.ends_with('…'));
    assert_eq!(fit("ok", &font, 200), "ok");
}
