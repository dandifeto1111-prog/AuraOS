//! Native Windows OLE External File Drag-and-Drop (`DoDragDrop` + `CF_HDROP`) & File Clipboard
//! - Enables dragging 1 or MULTIPLE files straight OUT of AuraFiles into ANY Windows application
//!   (Chrome, Edge, Discord, WhatsApp, Telegram, VS Code, Explorer, Desktop, AuraBeats, OBS, etc.)
//! - Also writes real `CF_HDROP` + `Preferred DropEffect` to the Windows Clipboard for cross-app Ctrl+C / Ctrl+X!

use crate::win32_shell::to_wide;
use std::mem::{size_of, ManuallyDrop};
use std::ptr;
use windows::core::{implement, Error, Result, HRESULT, PCWSTR};
use windows::Win32::Foundation::{
    GlobalFree, BOOL, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS,
    DV_E_FORMATETC, E_NOTIMPL, HANDLE, HGLOBAL, HWND, POINT, S_FALSE, S_OK,
};
use windows::Win32::System::Com::{
    IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, DATADIR_GET,
    DVASPECT_CONTENT, FORMATETC, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT,
};
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, OleInitialize, OleUninitialize, DROPEFFECT,
    DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::{DROPFILES, SHCreateStdEnumFmtEtc};

const CF_HDROP: u16 = 15;

pub fn init_ole() {
    unsafe {
        let _ = OleInitialize(None);
    }
}

pub fn uninit_ole() {
    unsafe {
        OleUninitialize();
    }
}

fn hdrop_formatetc() -> FORMATETC {
    FORMATETC {
        cfFormat: CF_HDROP,
        ptd: ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

/// Allocates a moveable HGLOBAL containing a UTF-16 `DROPFILES` header + double-null terminated paths.
fn build_hdrop_hglobal(paths: &[String]) -> Option<HGLOBAL> {
    if paths.is_empty() {
        return None;
    }
    let mut wide_chars: Vec<u16> = Vec::new();
    for p in paths {
        let trimmed = p.trim();
        if trimmed.is_empty() {
            continue;
        }
        wide_chars.extend(trimmed.encode_utf16());
        wide_chars.push(0);
    }
    if wide_chars.is_empty() {
        return None;
    }
    wide_chars.push(0); // Final double-null terminator required by CF_HDROP

    let hdr_size = size_of::<DROPFILES>();
    let str_bytes = wide_chars.len() * size_of::<u16>();
    let total = hdr_size + str_bytes;

    unsafe {
        let hmem = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, total).ok()?;
        let ptr_raw = GlobalLock(hmem) as *mut u8;
        if ptr_raw.is_null() {
            let _ = GlobalFree(hmem);
            return None;
        }
        let df = DROPFILES {
            pFiles: hdr_size as u32,
            pt: POINT { x: 0, y: 0 },
            fNC: BOOL(0),
            fWide: BOOL(1),
        };
        ptr::copy_nonoverlapping(&df as *const DROPFILES as *const u8, ptr_raw, hdr_size);
        ptr::copy_nonoverlapping(
            wide_chars.as_ptr() as *const u8,
            ptr_raw.add(hdr_size),
            str_bytes,
        );
        let _ = GlobalUnlock(hmem);
        Some(hmem)
    }
}

#[implement(IDataObject)]
struct FileListDataObject {
    paths: Vec<String>,
}

impl IDataObject_Impl for FileListDataObject_Impl {
    fn GetData(&self, pformatetcin: *const FORMATETC) -> Result<STGMEDIUM> {
        if pformatetcin.is_null() {
            return Err(Error::from(DV_E_FORMATETC));
        }
        let fmt = unsafe { &*pformatetcin };
        let wants_hdrop = fmt.cfFormat == CF_HDROP
            && (fmt.tymed & (TYMED_HGLOBAL.0 as u32)) != 0
            && (fmt.dwAspect == DVASPECT_CONTENT.0 || fmt.dwAspect == 0);
        if !wants_hdrop {
            return Err(Error::from(DV_E_FORMATETC));
        }
        let hglobal = build_hdrop_hglobal(&self.paths).ok_or_else(|| Error::from(DV_E_FORMATETC))?;
        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: hglobal },
            pUnkForRelease: ManuallyDrop::new(None),
        })
    }

    fn GetDataHere(&self, _pformatetc: *const FORMATETC, _pmedium: *mut STGMEDIUM) -> Result<()> {
        Err(Error::from(E_NOTIMPL))
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        if pformatetc.is_null() {
            return DV_E_FORMATETC;
        }
        let fmt = unsafe { &*pformatetc };
        if fmt.cfFormat == CF_HDROP
            && (fmt.tymed & (TYMED_HGLOBAL.0 as u32)) != 0
            && (fmt.dwAspect == DVASPECT_CONTENT.0 || fmt.dwAspect == 0)
        {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        _pformatectin: *const FORMATETC,
        pformatetcout: *mut FORMATETC,
    ) -> HRESULT {
        if !pformatetcout.is_null() {
            unsafe {
                (*pformatetcout).ptd = ptr::null_mut();
            }
        }
        windows::Win32::Foundation::DATA_S_SAMEFORMATETC
    }

    fn SetData(
        &self,
        _pformatetc: *const FORMATETC,
        _pmedium: *const STGMEDIUM,
        _frelease: BOOL,
    ) -> Result<()> {
        Err(Error::from(E_NOTIMPL))
    }

    fn EnumFormatEtc(&self, dwdirection: u32) -> Result<IEnumFORMATETC> {
        if dwdirection != DATADIR_GET.0 as u32 {
            return Err(Error::from(E_NOTIMPL));
        }
        let formats = [hdrop_formatetc()];
        unsafe { SHCreateStdEnumFmtEtc(&formats) }
    }

    fn DAdvise(
        &self,
        _pformatetc: *const FORMATETC,
        _advf: u32,
        _padvsink: Option<&IAdviseSink>,
    ) -> Result<u32> {
        Err(Error::from(windows::Win32::Foundation::OLE_E_ADVISENOTSUPPORTED))
    }

    fn DUnadvise(&self, _dwconnection: u32) -> Result<()> {
        Err(Error::from(windows::Win32::Foundation::OLE_E_ADVISENOTSUPPORTED))
    }

    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(Error::from(windows::Win32::Foundation::OLE_E_ADVISENOTSUPPORTED))
    }
}

#[implement(IDropSource)]
struct FileDropSource;

impl IDropSource_Impl for FileDropSource_Impl {
    fn QueryContinueDrag(&self, fescapepressed: BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        if fescapepressed.as_bool() {
            return DRAGDROP_S_CANCEL;
        }
        if (grfkeystate.0 & MK_LBUTTON.0) == 0 {
            return DRAGDROP_S_DROP;
        }
        S_OK
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOutcome {
    Cancelled,
    Copied,
    Moved,
    Linked,
}

/// Runs a native Windows OLE `DoDragDrop` session carrying `paths` as `CF_HDROP`.
/// IMPORTANT: Must NEVER be called while holding a `RefCell` borrow on the window runtime!
pub fn start_external_file_drag(paths: &[String]) -> DragOutcome {
    if paths.is_empty() {
        return DragOutcome::Cancelled;
    }
    let data_obj: IDataObject = FileListDataObject { paths: paths.to_vec() }.into();
    let drop_src: IDropSource = FileDropSource.into();
    let allowed = DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK;
    let mut effect = DROPEFFECT_NONE;

    let hr = unsafe { DoDragDrop(&data_obj, &drop_src, allowed, &mut effect) };
    if hr == DRAGDROP_S_DROP {
        if (effect.0 & DROPEFFECT_MOVE.0) != 0 {
            DragOutcome::Moved
        } else if (effect.0 & DROPEFFECT_LINK.0) != 0 {
            DragOutcome::Linked
        } else if (effect.0 & DROPEFFECT_COPY.0) != 0 {
            DragOutcome::Copied
        } else {
            // Some external apps (e.g. Chrome/Discord) accept CF_HDROP and leave effect=0 or COPY
            DragOutcome::Copied
        }
    } else if hr == S_FALSE || hr == DRAGDROP_S_CANCEL {
        DragOutcome::Cancelled
    } else {
        DragOutcome::Cancelled
    }
}

/// Copies 1 or more files to the native Windows Clipboard (`CF_HDROP` + `Preferred DropEffect`)
/// so `Ctrl+V` works in Discord, Explorer, Desktop, Chrome, WhatsApp, and AuraFiles!
pub fn copy_files_to_clipboard(paths: &[String], is_cut: bool) -> bool {
    let Some(hdrop) = build_hdrop_hglobal(paths) else {
        return false;
    };
    unsafe {
        if OpenClipboard(HWND::default()).is_err() {
            let _ = GlobalFree(hdrop);
            return false;
        }
        let _ = EmptyClipboard();
        let ok = SetClipboardData(CF_HDROP as u32, HANDLE(hdrop.0)).is_ok();
        if !ok {
            let _ = GlobalFree(hdrop);
            let _ = CloseClipboard();
            return false;
        }
        // Also write Preferred DropEffect (Copy = 1, Move = 2) so Windows Explorer honors Ctrl+X vs Ctrl+C!
        let pref_name = to_wide("Preferred DropEffect");
        let cf_pref = RegisterClipboardFormatW(PCWSTR(pref_name.as_ptr()));
        if cf_pref != 0 {
            if let Ok(heff) = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, 4) {
                let p = GlobalLock(heff) as *mut u32;
                if !p.is_null() {
                    *p = if is_cut { DROPEFFECT_MOVE.0 } else { DROPEFFECT_COPY.0 };
                    let _ = GlobalUnlock(heff);
                    if SetClipboardData(cf_pref, HANDLE(heff.0)).is_err() {
                        let _ = GlobalFree(heff);
                    }
                } else {
                    let _ = GlobalFree(heff);
                }
            }
        }
        let _ = CloseClipboard();
        true
    }
}
