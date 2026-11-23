//! Native Windows 11 Network & Radio Telemetry + WinRT In-Place Toggles
//! - Accurately distinguishes Ethernet vs Wi-Fi (with real 0-100% signal bars & SSID via wlanapi.dll)
//!   vs No Internet / Offline (via GetBestInterface + GetAdaptersAddresses gateway check!)
//! - Instant Wi-Fi & Bluetooth toggles via WinRT Windows.Devices.Radios.Radio (0 UAC prompts!)

use std::ffi::c_void;
use windows::core::{GUID, PCSTR, PCWSTR};
use windows::Devices::Radios::{Radio, RadioKind, RadioState};
use windows::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GetBestInterface, GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST,
    GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Foundation::FreeLibrary;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetKind {
    Ethernet,
    Wifi,
    #[default]
    Offline,
}

#[derive(Debug, Clone, Default)]
pub struct RadioStates {
    pub wifi_available: bool,
    pub wifi_on: bool,
    pub bt_available: bool,
    pub bt_on: bool,
    pub ethernet_up: bool,
    pub has_internet: bool,
    pub net_kind: NetKind,
    pub wifi_signal_pct: u8,
    pub wifi_bars: u8, // 0..=4
    pub wifi_ssid: String,
    pub net_label: String,
}

// ==================== DYNAMIC WLANAPI.DLL (SIGNAL BARS + SSID, ZERO EXTRA CRATES) ====================

#[repr(C)]
struct WlanInterfaceInfo {
    interface_guid: GUID,
    description: [u16; 256],
    state: u32, // 1 = wlan_interface_state_connected
}

#[repr(C)]
struct WlanInterfaceInfoList {
    number_of_items: u32,
    index: u32,
    interface_info: [WlanInterfaceInfo; 1],
}

#[repr(C)]
struct Dot11Ssid {
    ssid_length: u32,
    ssid: [u8; 32],
}

#[repr(C)]
struct WlanAssociationAttributes {
    dot11_ssid: Dot11Ssid,
    dot11_bss_type: u32,
    dot11_bssid: [u8; 6],
    _pad0: [u8; 2],
    dot11_phy_type: u32,
    phy_index: u32,
    wlan_signal_quality: u32, // 0..=100
    rx_rate: u32,
    tx_rate: u32,
}

#[repr(C)]
struct WlanConnectionAttributes {
    is_state: u32,
    wlan_connection_mode: u32,
    profile_name: [u16; 256],
    wlan_association_attributes: WlanAssociationAttributes,
    // followed by security attributes (unused)
}

type FnWlanOpenHandle = unsafe extern "system" fn(u32, *mut c_void, *mut u32, *mut *mut c_void) -> u32;
type FnWlanCloseHandle = unsafe extern "system" fn(*mut c_void, *mut c_void) -> u32;
type FnWlanEnumInterfaces = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut WlanInterfaceInfoList) -> u32;
type FnWlanQueryInterface = unsafe extern "system" fn(*mut c_void, *const GUID, u32, *mut c_void, *mut u32, *mut *mut c_void, *mut u32) -> u32;
type FnWlanFreeMemory = unsafe extern "system" fn(*mut c_void);

/// Queries real connected Wi-Fi SSID and Signal Quality (0..=100%) using native wlanapi.dll!
fn query_wlan_signal() -> Option<(String, u8)> {
    unsafe {
        let dll_name: Vec<u16> = "wlanapi.dll\0".encode_utf16().collect();
        let hmod = LoadLibraryW(PCWSTR(dll_name.as_ptr())).ok()?;

        let open_fn = GetProcAddress(hmod, PCSTR(b"WlanOpenHandle\0".as_ptr()));
        let close_fn = GetProcAddress(hmod, PCSTR(b"WlanCloseHandle\0".as_ptr()));
        let enum_fn = GetProcAddress(hmod, PCSTR(b"WlanEnumInterfaces\0".as_ptr()));
        let query_fn = GetProcAddress(hmod, PCSTR(b"WlanQueryInterface\0".as_ptr()));
        let free_fn = GetProcAddress(hmod, PCSTR(b"WlanFreeMemory\0".as_ptr()));

        let (Some(f_open), Some(f_close), Some(f_enum), Some(f_query), Some(f_free)) =
            (open_fn, close_fn, enum_fn, query_fn, free_fn)
        else {
            let _ = FreeLibrary(hmod);
            return None;
        };

        let wlan_open: FnWlanOpenHandle = std::mem::transmute(f_open);
        let wlan_close: FnWlanCloseHandle = std::mem::transmute(f_close);
        let wlan_enum: FnWlanEnumInterfaces = std::mem::transmute(f_enum);
        let wlan_query: FnWlanQueryInterface = std::mem::transmute(f_query);
        let wlan_free: FnWlanFreeMemory = std::mem::transmute(f_free);

        let mut neg_ver = 0u32;
        let mut client_handle: *mut c_void = std::ptr::null_mut();
        if wlan_open(2, std::ptr::null_mut(), &mut neg_ver, &mut client_handle) != 0 || client_handle.is_null() {
            let _ = FreeLibrary(hmod);
            return None;
        }

        let mut result: Option<(String, u8)> = None;
        let mut list_ptr: *mut WlanInterfaceInfoList = std::ptr::null_mut();
        if wlan_enum(client_handle, std::ptr::null_mut(), &mut list_ptr) == 0 && !list_ptr.is_null() {
            let count = (*list_ptr).number_of_items as usize;
            let base = &(*list_ptr).interface_info as *const WlanInterfaceInfo;
            for i in 0..count {
                let info = &*base.add(i);
                if info.state == 1 {
                    // wlan_intf_opcode_current_connection = 7
                    let mut data_size = 0u32;
                    let mut data_ptr: *mut c_void = std::ptr::null_mut();
                    let mut opcode_val_type = 0u32;
                    if wlan_query(
                        client_handle,
                        &info.interface_guid,
                        7,
                        std::ptr::null_mut(),
                        &mut data_size,
                        &mut data_ptr,
                        &mut opcode_val_type,
                    ) == 0
                        && !data_ptr.is_null()
                        && (data_size as usize) >= std::mem::size_of::<WlanConnectionAttributes>()
                    {
                        let conn = &*(data_ptr as *const WlanConnectionAttributes);
                        let sig = conn.wlan_association_attributes.wlan_signal_quality.min(100) as u8;
                        let ssid_len = (conn.wlan_association_attributes.dot11_ssid.ssid_length as usize).min(32);
                        let ssid = String::from_utf8_lossy(&conn.wlan_association_attributes.dot11_ssid.ssid[..ssid_len]).trim().to_string();
                        result = Some((ssid, sig));
                        wlan_free(data_ptr);
                        break;
                    }
                    if !data_ptr.is_null() {
                        wlan_free(data_ptr);
                    }
                }
            }
            wlan_free(list_ptr as *mut c_void);
        }

        wlan_close(client_handle, std::ptr::null_mut());
        let _ = FreeLibrary(hmod);
        result
    }
}

fn is_virtual_adapter(desc: &str) -> bool {
    let low = desc.to_lowercase();
    low.contains("virtual")
        || low.contains("vethernet")
        || low.contains("hyper-v")
        || low.contains("vmware")
        || low.contains("loopback")
        || low.contains("tap-")
        || low.contains("tailscale")
        || low.contains("zerotier")
        || low.contains("radmin")
        || low.contains("hamachi")
        || low.contains("wintun")
        || low.contains("bluetooth")
}

/// Detects real active Internet route & adapter type (Ethernet vs Wi-Fi vs Offline)!
fn detect_active_network() -> (NetKind, bool, bool) {
    unsafe {
        // 1. Ask Windows IP routing table which exact interface routes traffic to public IP 1.1.1.1!
        let mut best_if_idx = 0u32;
        let has_route = GetBestInterface(0x01010101u32, &mut best_if_idx) == 0 && best_if_idx != 0;

        let mut size = 32768u32;
        let mut buf = vec![0u8; size as usize];
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_INCLUDE_GATEWAYS;
        let mut ret = GetAdaptersAddresses(
            0,
            flags,
            None,
            Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
            &mut size,
        );
        if ret == 111 && size > 0 {
            // ERROR_BUFFER_OVERFLOW: grow buffer to exact needed size
            buf.resize(size as usize, 0);
            ret = GetAdaptersAddresses(
                0,
                flags,
                None,
                Some(buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                &mut size,
            );
        }
        if ret != 0 {
            return (NetKind::Offline, false, false);
        }

        let mut eth_up = false;
        let mut wifi_up = false;
        let mut best_kind: Option<NetKind> = None;

        let mut ptr = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !ptr.is_null() {
            let a = &*ptr;
            if a.OperStatus == IfOperStatusUp {
                let desc = a.Description.to_string().unwrap_or_default();
                let has_gw = !a.FirstGatewayAddress.is_null();
                if has_gw && !is_virtual_adapter(&desc) {
                    let if_idx = a.Anonymous1.Anonymous.IfIndex;
                    if a.IfType == 6 {
                        eth_up = true;
                        if has_route && if_idx == best_if_idx {
                            best_kind = Some(NetKind::Ethernet);
                        }
                    } else if a.IfType == 71 {
                        wifi_up = true;
                        if has_route && if_idx == best_if_idx {
                            best_kind = Some(NetKind::Wifi);
                        }
                    }
                }
            }
            ptr = a.Next;
        }

        if let Some(k) = best_kind {
            return (k, eth_up, true);
        }
        if eth_up && has_route {
            return (NetKind::Ethernet, true, true);
        }
        if wifi_up && has_route {
            return (NetKind::Wifi, eth_up, true);
        }
        (NetKind::Offline, eth_up, false)
    }
}

pub fn signal_pct_to_bars(pct: u8) -> u8 {
    match pct {
        0 => 0,
        1..=25 => 1,
        26..=55 => 2,
        56..=80 => 3,
        _ => 4,
    }
}

pub fn query_radio_states() -> RadioStates {
    let mut st = RadioStates::default();

    let (net_kind, eth_up, has_internet) = detect_active_network();
    st.net_kind = net_kind;
    st.ethernet_up = eth_up;
    st.has_internet = has_internet;

    if let Some((ssid, pct)) = query_wlan_signal() {
        st.wifi_ssid = ssid;
        st.wifi_signal_pct = pct;
        st.wifi_bars = signal_pct_to_bars(pct);
    }

    // Query real Windows 11 hardware radios via WinRT Radio::GetRadiosAsync()
    if let Ok(op) = Radio::GetRadiosAsync() {
        if let Ok(radios) = op.get() {
            if let Ok(count) = radios.Size() {
                for i in 0..count {
                    if let Ok(r) = radios.GetAt(i) {
                        let kind = r.Kind().unwrap_or(RadioKind::Other);
                        let state = r.State().unwrap_or(RadioState::Unknown);
                        match kind {
                            RadioKind::WiFi => {
                                st.wifi_available = true;
                                st.wifi_on = state == RadioState::On;
                            }
                            RadioKind::Bluetooth => {
                                st.bt_available = true;
                                st.bt_on = state == RadioState::On;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    st.net_label = match st.net_kind {
        NetKind::Ethernet => {
            if st.wifi_on && !st.wifi_ssid.is_empty() {
                format!("Ethernet · Wi-Fi ({}%)", st.wifi_signal_pct)
            } else {
                "Ethernet conectado".to_string()
            }
        }
        NetKind::Wifi => {
            if !st.wifi_ssid.is_empty() {
                format!("{} · {}%", st.wifi_ssid, st.wifi_signal_pct)
            } else {
                format!("Wi-Fi · {}%", st.wifi_signal_pct)
            }
        }
        NetKind::Offline => {
            if st.wifi_available && !st.wifi_on {
                "Sin internet · Wi-Fi Off".to_string()
            } else {
                "Sin conexión a Internet".to_string()
            }
        }
    };

    st
}

pub fn toggle_radio(target_kind: RadioKind) -> Option<bool> {
    let op = Radio::GetRadiosAsync().ok()?;
    let radios = op.get().ok()?;
    let count = radios.Size().ok()?;
    for i in 0..count {
        if let Ok(r) = radios.GetAt(i) {
            if r.Kind().ok() == Some(target_kind) {
                let cur_on = r.State().ok() == Some(RadioState::On);
                let next_state = if cur_on { RadioState::Off } else { RadioState::On };
                if let Ok(set_op) = r.SetStateAsync(next_state) {
                    let _ = set_op.get();
                    return Some(!cur_on);
                }
            }
        }
    }
    None
}

pub fn toggle_wifi() -> Option<bool> {
    toggle_radio(RadioKind::WiFi)
}

pub fn toggle_bluetooth() -> Option<bool> {
    toggle_radio(RadioKind::Bluetooth)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_query_radio_states() {
        let t0 = std::time::Instant::now();
        let net = detect_active_network();
        println!("detect_active_network={:?} in {:?}", net, t0.elapsed());
        let t1 = std::time::Instant::now();
        let wlan = query_wlan_signal();
        println!("query_wlan_signal={:?} in {:?}", wlan, t1.elapsed());
        let t2 = std::time::Instant::now();
        let st = query_radio_states();
        println!("query_radio_states={:?} in {:?}", st, t2.elapsed());
    }
}
