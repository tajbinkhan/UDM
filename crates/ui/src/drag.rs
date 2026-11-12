//! Dragging a finished download's file out of the window, as a file manager
//! drags one: a drop in a folder copies it, and a program takes it as the
//! file itself. Moving is not offered, so the download keeps its file. Only
//! Windows so far ([`CAN_DRAG`]); elsewhere the window offers no drag.

use tauri::AppHandle;

/// Whether this system drags files out of the window.
pub const CAN_DRAG: bool = cfg!(windows);

/// Start dragging `path` out of the window `label`, while its left button
/// is down. The drag runs on the main thread, which OLE needs; its loop
/// goes on handling the windows' messages until the file is dropped or the
/// drag is given up, so the main thread does not stop answering.
pub fn start(app: &AppHandle, label: &str, path: String) {
    #[cfg(windows)]
    {
        use tauri::Manager;
        let Some(hwnd) = app.get_webview_window(label).and_then(|w| w.hwnd().ok()) else { return };
        let hwnd = hwnd.0 as isize;
        if let Err(e) = app.run_on_main_thread(move || win::drag(hwnd, &path)) {
            tracing::warn!(error = %e, "the drag did not start");
        }
    }
    #[cfg(not(windows))]
    let _ = (app, label, path);
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    use ::windows::core::HSTRING;
    use ::windows::Win32::Foundation::HWND;
    use ::windows::Win32::System::Com::{IBindCtx, IDataObject};
    use ::windows::Win32::System::Ole::{
        IDropSource, OleInitialize, OleUninitialize, DROPEFFECT_COPY, DROPEFFECT_LINK,
    };
    use ::windows::Win32::UI::Shell::{BHID_DataObject, IShellItem, SHCreateItemFromParsingName, SHDoDragDrop};

    /// Drag the file at `path` out of the window `hwnd`. It returns once
    /// the file is dropped or the drag is given up.
    pub fn drag(hwnd: isize, path: &str) {
        // SAFETY: COM calls on objects made here, on the main thread, which
        // OLE needs for a drag. The shell supplies both the file's data
        // object and the drop source, and runs the drag's loop until it
        // ends.
        unsafe {
            let ole = OleInitialize(None).is_ok();
            if let Some(data) = file_data(path) {
                let _ = SHDoDragDrop(
                    Some(HWND(hwnd as *mut c_void)),
                    &data,
                    None::<&IDropSource>,
                    DROPEFFECT_COPY | DROPEFFECT_LINK,
                );
            }
            if ole {
                OleUninitialize();
            }
        }
    }

    /// The shell's data object for the file at `path`, the one Explorer
    /// drags, which carries it as a file list (`CF_HDROP`) among other
    /// formats.
    fn file_data(path: &str) -> Option<IDataObject> {
        // SAFETY: the path lives until the call returns; the item and its
        // data object are COM objects that release themselves when dropped.
        unsafe {
            let item = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(path), None::<&IBindCtx>).ok()?;
            item.BindToHandler::<_, IDataObject>(None::<&IBindCtx>, &BHID_DataObject).ok()
        }
    }
}
