use std::{
    fmt::Write,
    mem::{size_of_val, transmute, zeroed},
    sync::Arc,
};

use anyhow::{anyhow, ensure, Result};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::{core::*, Win32::Graphics::Dxgi::CreateDXGIFactory1};

use super::{CaptureDxgi, CaptureStage, MonitorInfo, RefreshRate, Resolution};

pub struct CaptureFactoryWin32 {}

impl CaptureFactoryWin32 {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    pub fn list(&mut self) -> Vec<MonitorInfo> {
        unsafe {
            let mut output = vec![];

            let ok = EnumDisplayMonitors(
                None,
                None,
                Some(accessor),
                transmute::<&mut AccessorParam, LPARAM>(&mut output),
            );

            if !ok.as_bool() {
                println!("EnumDisplayMonitors failed");
            }

            output
        }
    }

    pub fn available_backends(&mut self) -> Vec<String> {
        //TODO: Use enum
        vec!["dxgi".into(), "gdi".into()]
    }

    pub fn start(&mut self, backend: &str, id: &str) -> Result<Arc<dyn CaptureStage>> {
        let dev_id = decode_hex(id)?;

        let factory = unsafe { CreateDXGIFactory1()? };

        match backend {
            "dxgi" => Ok(CaptureDxgi::new(factory, dev_id)?),
            "gdi" => panic!("gdi temporarily disabled"),
            _ => Err(anyhow!("no such capture backend available")),
        }
    }
}

type AccessorParam = Vec<MonitorInfo>;

/// Called by EnumDisplayMonitors
unsafe extern "system" fn accessor(hmonitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // Safe because this callback is called solely within EnumDisplayMonitors
    let output: &mut AccessorParam = transmute(data);

    let mut info: MONITORINFOEXW = zeroed();
    info.monitorInfo.cbSize = size_of_val(&info) as u32;

    let ok = GetMonitorInfoW(hmonitor, transmute(&mut info));
    if !ok.as_bool() {
        // skip if failed to get monitor info
        //TODO: A log would be nice
        return BOOL(1);
    }

    let refresh_rate = get_refresh_rate_dxgi(&info.szDevice)
        .or_else(|| get_refresh_rate_gdi(&info.szDevice))
        .unwrap_or_default();

    let rect = &info.monitorInfo.rcMonitor;
    let id = encode_hex(trim_end_null(&info.szDevice));
    let name = String::from_utf16_lossy(trim_end_null(&info.szDevice));
    let width = i32::abs(rect.right - rect.left) as u32;
    let height = i32::abs(rect.bottom - rect.top) as u32;

    output.push(MonitorInfo {
        id,
        name,
        resolution: Resolution { width, height },
        refresh_rate,
    });

    BOOL(1)
}

unsafe fn get_refresh_rate_dxgi(_dev_name: &[u16; 32]) -> Option<RefreshRate> {
    //TODO: Implement this
    None
}

/// This method produces inexact, rounded value. Prefer dxgi version whenever possible
unsafe fn get_refresh_rate_gdi(dev_name: &[u16; 32]) -> Option<RefreshRate> {
    let mut mode: DEVMODEW = zeroed();
    mode.dmSize = size_of_val(&mode) as u16;
    mode.dmDriverExtra = 0;
    let ok = EnumDisplaySettingsW(
        PCWSTR::from_raw(dev_name.as_ptr()),
        ENUM_CURRENT_SETTINGS,
        &mut mode,
    );

    if !ok.as_bool() {
        return None;
    }

    Some(RefreshRate {
        num: mode.dmDisplayFrequency,
        den: 1,
    })
}

/// Encode given number sequence into hex representation
fn encode_hex(s: &[u16]) -> String {
    let mut out = String::with_capacity(s.len() * 4);
    for num in s {
        write!(&mut out, "{num:04x}").expect("writing to string must not fail");
    }

    out
}

/// Opposite of encode_hex. Appends single NULL character at end
fn decode_hex(s: &str) -> Result<Vec<u16>> {
    ensure!(s.len() % 4 == 0, "invalid hex string");

    let mut output = Vec::with_capacity(s.len() / 4 + 1);

    for i in (0..s.len()).step_by(4) {
        let value = &s[i..i + 4];
        let value = u16::from_str_radix(value, 16)?;
        output.push(value);
    }

    output.push(0);

    Ok(output)
}

/// Trim the UTF-16 string to remove NULL at end. Works even if no NULL is found.
fn trim_end_null(s: &[u16]) -> &[u16] {
    for (i, x) in s.iter().enumerate() {
        if *x == 0 {
            return &s[0..i];
        }
    }

    // no NULL found
    s
}
