#[cfg(target_os = "windows")]
mod win {
    use std::ffi::c_void;

    type BOOL = i32;
    type DWORD = u32;
    type WORD = u16;
    #[allow(non_camel_case_types)]
    type ULONG_PTR = usize;
    type HGLOBAL = *mut c_void;
    type HDC = *mut c_void;
    type HBITMAP = *mut c_void;
    type HGDIOBJ = *mut c_void;
    type HWND = *mut c_void;
    type HMONITOR = *mut c_void;

    #[repr(C)]
    struct GdiplusStartupInput {
        gdiplus_version: u32,
        debug_event_callback: *const c_void,
        suppress_background_thread: BOOL,
        suppress_external_codecs: BOOL,
    }

    #[repr(C)]
    struct GUID {
        data1: DWORD,
        data2: WORD,
        data3: WORD,
        data4: [u8; 8],
    }

    #[repr(C)]
    #[derive(Copy, Clone, Default)]
    struct POINT {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    #[derive(Copy, Clone, Default)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct MONITORINFO {
        cb_size: DWORD,
        rc_monitor: RECT,
        rc_work: RECT,
        dw_flags: DWORD,
    }

    const SRCCOPY: DWORD = 0x00CC0020;
    const SM_CXSCREEN: i32 = 0;
    const SM_CYSCREEN: i32 = 1;
    const MONITOR_DEFAULTTONEAREST: DWORD = 2;

    // JPEG encoder CLSID: 557CF401-1A04-11D3-9A73-0000F81EF32E
    const CLSID_JPEG: GUID = GUID {
        data1: 0x557cf401,
        data2: 0x1a04,
        data3: 0x11d3,
        data4: [0x9a, 0x73, 0x00, 0x00, 0xf8, 0x1e, 0xf3, 0x2e],
    };

    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetSystemMetrics(nIndex: i32) -> i32;
        fn GetDC(hWnd: HWND) -> HDC;
        fn ReleaseDC(hWnd: HWND, hDC: HDC) -> i32;
        fn GetCursorPos(lpPoint: *mut POINT) -> BOOL;
        fn MonitorFromPoint(pt: POINT, dwFlags: DWORD) -> HMONITOR;
        fn GetMonitorInfoW(hMonitor: HMONITOR, lpmi: *mut MONITORINFO) -> BOOL;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateCompatibleDC(hdc: HDC) -> HDC;
        fn CreateCompatibleBitmap(hdc: HDC, cx: i32, cy: i32) -> HBITMAP;
        fn SelectObject(hdc: HDC, h: HGDIOBJ) -> HGDIOBJ;
        fn BitBlt(hdc: HDC, x: i32, y: i32, cx: i32, cy: i32, hdcSrc: HDC, x1: i32, y1: i32, rop: DWORD) -> BOOL;
        fn DeleteObject(ho: HGDIOBJ) -> BOOL;
        fn DeleteDC(hdc: HDC) -> BOOL;
    }

    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CreateStreamOnHGlobal(hGlobal: HGLOBAL, fDeleteOnRelease: BOOL, ppstm: *mut *mut c_void) -> i32;
        fn GetHGlobalFromStream(pstm: *mut c_void, phglobal: *mut HGLOBAL) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalSize(hMem: HGLOBAL) -> usize;
        fn GlobalLock(hMem: HGLOBAL) -> *mut c_void;
        fn GlobalUnlock(hMem: HGLOBAL) -> BOOL;
    }

    #[link(name = "gdiplus")]
    unsafe extern "system" {
        fn GdiplusStartup(token: *mut ULONG_PTR, input: *const GdiplusStartupInput, output: *mut c_void) -> i32;
        fn GdiplusShutdown(token: ULONG_PTR);
        fn GdipCreateBitmapFromHBITMAP(hbm: HBITMAP, hpal: *mut c_void, bitmap: *mut *mut c_void) -> i32;
        fn GdipSaveImageToStream(image: *mut c_void, stream: *mut c_void, clsidEncoder: *const GUID, encoderParams: *const c_void) -> i32;
        fn GdipDisposeImage(image: *mut c_void) -> i32;
    }

    pub fn capture_screen_jpeg_base64() -> Result<String, String> {
        unsafe {
            let mut token: ULONG_PTR = 0;
            let input = GdiplusStartupInput {
                gdiplus_version: 1,
                debug_event_callback: std::ptr::null(),
                suppress_background_thread: 0,
                suppress_external_codecs: 0,
            };

            let status = GdiplusStartup(&mut token, &input, std::ptr::null_mut());
            if status != 0 {
                return Err("Failed to initialize Windows graphics pipeline.".into());
            }

            // Detect the monitor where the candidate's mouse/focus is located
            // This ensures multi-monitor setups (e.g. laptop + external monitor with LeetCode/IDE) are captured accurately!
            let mut pt = POINT::default();
            let mut capture_x = 0;
            let mut capture_y = 0;
            let mut width = GetSystemMetrics(SM_CXSCREEN);
            let mut height = GetSystemMetrics(SM_CYSCREEN);

            if GetCursorPos(&mut pt) != 0 {
                let h_mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
                if !h_mon.is_null() {
                    let mut mi: MONITORINFO = std::mem::zeroed();
                    mi.cb_size = std::mem::size_of::<MONITORINFO>() as DWORD;
                    if GetMonitorInfoW(h_mon, &mut mi) != 0 {
                        let mon_w = mi.rc_monitor.right - mi.rc_monitor.left;
                        let mon_h = mi.rc_monitor.bottom - mi.rc_monitor.top;
                        if mon_w > 0 && mon_h > 0 {
                            capture_x = mi.rc_monitor.left;
                            capture_y = mi.rc_monitor.top;
                            width = mon_w;
                            height = mon_h;
                        }
                    }
                }
            }

            if width <= 0 || height <= 0 {
                GdiplusShutdown(token);
                return Err("Invalid screen metrics.".into());
            }

            let hdc_screen = GetDC(std::ptr::null_mut());
            if hdc_screen.is_null() {
                GdiplusShutdown(token);
                return Err("Could not acquire desktop device context.".into());
            }

            let hdc_mem = CreateCompatibleDC(hdc_screen);
            let hbitmap = CreateCompatibleBitmap(hdc_screen, width, height);
            let old_bmp = SelectObject(hdc_mem, hbitmap);

            // Silent BitBlt directly from DWM framebuffer in RAM (zero focus shift, zero flicker)
            let blt_res = BitBlt(hdc_mem, 0, 0, width, height, hdc_screen, capture_x, capture_y, SRCCOPY);

            let mut gp_bitmap: *mut c_void = std::ptr::null_mut();
            let mut stream: *mut c_void = std::ptr::null_mut();
            let mut result_base64 = Err("Capture failed.".to_string());

            let bmp_status = GdipCreateBitmapFromHBITMAP(hbitmap, std::ptr::null_mut(), &mut gp_bitmap);
            let stream_status = CreateStreamOnHGlobal(std::ptr::null_mut(), 1, &mut stream);
            let save_status = if stream_status == 0 && !gp_bitmap.is_null() {
                GdipSaveImageToStream(gp_bitmap, stream, &CLSID_JPEG, std::ptr::null())
            } else {
                -1
            };

            if save_status == 0 {
                let mut hglobal: HGLOBAL = std::ptr::null_mut();
                if GetHGlobalFromStream(stream, &mut hglobal) == 0 && !hglobal.is_null() {
                    let size = GlobalSize(hglobal);
                    let ptr = GlobalLock(hglobal);
                    if !ptr.is_null() && size > 0 {
                        let slice = std::slice::from_raw_parts(ptr as *const u8, size);
                        use base64::Engine;
                        let encoded = base64::engine::general_purpose::STANDARD.encode(slice);
                        result_base64 = Ok(format!("data:image/jpeg;base64,{encoded}"));
                        GlobalUnlock(hglobal);
                    }
                }
            } else {
                result_base64 = Err(format!("Capture failed: blt={blt_res}, bmp_st={bmp_status}, stm_st={stream_status}, save_st={save_status}"));
            }

            // Cleanup COM IStream interface to prevent memory leaks across repeated snaps
            if !stream.is_null() {
                let vtbl = *(stream as *mut *mut extern "system" fn(*mut c_void) -> u32);
                let release_fn = *vtbl.offset(2);
                release_fn(stream);
            }

            // Cleanup GDI and GDI+ resources
            if !gp_bitmap.is_null() {
                GdipDisposeImage(gp_bitmap);
            }
            SelectObject(hdc_mem, old_bmp);
            DeleteObject(hbitmap);
            DeleteDC(hdc_mem);
            ReleaseDC(std::ptr::null_mut(), hdc_screen);
            GdiplusShutdown(token);

            result_base64
        }
    }
}

pub fn capture_screen_silent() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        win::capture_screen_jpeg_base64()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("Silent screen capture is currently implemented for Windows.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    fn test_silent_screen_capture_produces_valid_jpeg() {
        let result = capture_screen_silent();
        assert!(result.is_ok(), "Silent capture failed: {:?}", result.err());
        let data_url = result.unwrap();
        assert!(data_url.starts_with("data:image/jpeg;base64,"));
        let b64 = &data_url["data:image/jpeg;base64,".len()..];
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).expect("Valid base64");
        assert!(bytes.len() > 1000, "Image too small: {} bytes", bytes.len());
        // Verify JPEG magic bytes FF D8 FF
        assert_eq!(&bytes[0..3], &[0xFF, 0xD8, 0xFF], "Must have JPEG magic bytes");
    }
}
