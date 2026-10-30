//! What makes the desktop change, each on a thread blocked on its source:
//!
//! - the root picture and the screen size: an X connection waiting for
//!   `_XROOTPMAP_ID` property changes and root `ConfigureNotify`; the session
//!   and Control Panels paint the picture, the desktop shows a copy;
//! - the items: inotify on `~/Desktop` and the Trash, and `POLLPRI` on
//!   `/proc/self/mountinfo`, which the kernel raises when mounts change.
//!
//! Each change is sent once a burst of events has been read, then `wake`
//! runs. Nothing polls on a timer.

use std::ffi::CString;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;

use slopos_ui::image::Image;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConnectionExt, EventMask, ImageFormat, ImageOrder,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

pub enum Change {
    /// The screen's size in device pixels and a copy of the root picture.
    Background {
        size: (u32, u32),
        picture: Option<Image>,
    },
    /// Something in ~/Desktop, the Trash or the mounts changed.
    Items,
}

type Wake = Arc<dyn Fn() + Send + Sync>;

/// Copy of the root window's background pixmap, if the session set one.
fn root_picture(conn: &RustConnection, root: u32, atom: u32, size: (u32, u32)) -> Option<Image> {
    let pixmap = conn
        .get_property(false, root, atom, AtomEnum::PIXMAP, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()
        .filter(|&id| id != 0)?;
    let geometry = conn.get_geometry(pixmap).ok()?.reply().ok()?;
    let (width, height) = (
        u32::from(geometry.width).min(size.0),
        u32::from(geometry.height).min(size.1),
    );
    let reply = conn
        .get_image(
            ImageFormat::Z_PIXMAP,
            pixmap,
            0,
            0,
            width as u16,
            height as u16,
            !0,
        )
        .ok()?
        .reply()
        .ok()?;
    let lsb = conn.setup().image_byte_order == ImageOrder::LSB_FIRST;
    if reply.depth < 24 || reply.data.len() < (width * height * 4) as usize {
        return None;
    }
    bgrx_to_image(&reply.data, width, height, lsb)
}

/// 32-bit ZPixmap data (BGRX in LSB-first order) as an opaque image.
pub fn bgrx_to_image(data: &[u8], width: u32, height: u32, lsb_first: bool) -> Option<Image> {
    let pixels = (width as usize).checked_mul(height as usize)?;
    let mut rgba = Vec::with_capacity(pixels * 4);
    for pixel in data.as_chunks::<4>().0.iter().take(pixels) {
        let (r, g, b) = if lsb_first {
            (pixel[2], pixel[1], pixel[0])
        } else {
            (pixel[1], pixel[2], pixel[3])
        };
        rgba.extend_from_slice(&[r, g, b, 255]);
    }
    Image::from_rgba(width, height, rgba).ok()
}

/// Follow the root picture and the screen size.
pub fn watch_root(sender: Sender<Change>, wake: Wake) -> Option<()> {
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots[screen].root;
    let atom = conn
        .intern_atom(false, b"_XROOTPMAP_ID")
        .ok()?
        .reply()
        .ok()?
        .atom;
    conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new()
            .event_mask(EventMask::PROPERTY_CHANGE | EventMask::STRUCTURE_NOTIFY),
    )
    .ok()?;
    conn.flush().ok()?;
    thread::Builder::new()
        .name("desktop-root".into())
        .spawn(move || loop {
            let size = conn
                .get_geometry(root)
                .ok()
                .and_then(|cookie| cookie.reply().ok())
                .map_or((0, 0), |g| (u32::from(g.width), u32::from(g.height)));
            let picture = root_picture(&conn, root, atom, size);
            if sender.send(Change::Background { size, picture }).is_err() {
                return;
            }
            wake();
            // Wait for a change that matters, then read the burst after it.
            loop {
                let Ok(event) = conn.wait_for_event() else {
                    return;
                };
                let relevant = match event {
                    Event::PropertyNotify(notify) => notify.atom == atom,
                    Event::ConfigureNotify(notify) => notify.window == root,
                    _ => false,
                };
                if relevant {
                    while let Ok(Some(_)) = conn.poll_for_event() {}
                    break;
                }
            }
        })
        .ok()?;
    Some(())
}

const WATCH_MASK: u32 = libc::IN_CREATE
    | libc::IN_DELETE
    | libc::IN_MOVED_FROM
    | libc::IN_MOVED_TO
    | libc::IN_ATTRIB
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF;

fn add_watch(fd: i32, path: &Path) {
    if let Ok(path) = CString::new(path.as_os_str().as_bytes()) {
        // SAFETY: valid inotify descriptor and NUL-terminated path.
        unsafe { libc::inotify_add_watch(fd, path.as_ptr(), WATCH_MASK) };
    }
}

/// Follow `~/Desktop`, the Trash's files and the mount table.
pub fn watch_items(desktop: PathBuf, trash: PathBuf, sender: Sender<Change>, wake: Wake) {
    let _ = thread::Builder::new()
        .name("desktop-items".into())
        .spawn(move || {
            // SAFETY: plain syscall; the descriptor is owned below.
            let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
            if fd < 0 {
                log::warn!("desktop: no inotify; the icons won't refresh by themselves");
                return;
            }
            let _ = std::fs::create_dir_all(&trash);
            add_watch(fd, &desktop);
            add_watch(fd, &trash);
            let mounts = std::fs::File::open("/proc/self/mountinfo").ok();
            let mut buffer = [0u8; 4096];
            loop {
                let mut fds = vec![libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                }];
                if let Some(file) = &mounts {
                    fds.push(libc::pollfd {
                        fd: file.as_raw_fd(),
                        events: libc::POLLPRI,
                        revents: 0,
                    });
                }
                // SAFETY: `fds` lives across the call.
                let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
                if ready < 0 {
                    if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    return;
                }
                if fds[0].revents & libc::POLLIN != 0 {
                    // Read everything queued; the contents don't matter.
                    loop {
                        // SAFETY: reading into our own buffer.
                        let read =
                            unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
                        if read <= 0 {
                            break;
                        }
                        let mut more = libc::pollfd {
                            fd,
                            events: libc::POLLIN,
                            revents: 0,
                        };
                        // SAFETY: one pollfd, zero timeout.
                        if unsafe { libc::poll(&mut more, 1, 0) } <= 0 {
                            break;
                        }
                    }
                    // A replaced ~/Desktop needs its watch again.
                    add_watch(fd, &desktop);
                }
                if let (Some(file), Some(pollfd)) = (&mounts, fds.get(1)) {
                    if pollfd.revents & (libc::POLLPRI | libc::POLLERR) != 0 {
                        // Re-reading from the start clears the mount event.
                        // SAFETY: seeking our own descriptor.
                        unsafe { libc::lseek(file.as_raw_fd(), 0, libc::SEEK_SET) };
                        let _ = std::io::Read::read(&mut &*file, &mut buffer);
                    }
                }
                if sender.send(Change::Items).is_err() {
                    return;
                }
                wake();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_pixels_become_an_opaque_image() {
        let data = [0x10, 0x20, 0x30, 0x00, 0xff, 0x00, 0x00, 0x00];
        let image = bgrx_to_image(&data, 2, 1, true).unwrap();
        assert_eq!((image.width(), image.height()), (2, 1));
        assert_eq!(
            &image.data()[..8],
            &[0x30, 0x20, 0x10, 255, 0, 0, 0xff, 255]
        );
        assert!(bgrx_to_image(&data, 3, 1, true).is_none());
        assert!(bgrx_to_image(&[], 0, 0, true).is_none());
    }

    #[test]
    fn desktop_changes_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let desktop = dir.path().join("Desktop");
        std::fs::create_dir(&desktop).unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        watch_items(
            desktop.clone(),
            dir.path().join("Trash/files"),
            sender,
            Arc::new(|| {}),
        );
        // Give the thread its watch before changing anything.
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::write(desktop.join("note.txt"), "x").unwrap();
        let change = receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(matches!(change, Change::Items));
    }
}
