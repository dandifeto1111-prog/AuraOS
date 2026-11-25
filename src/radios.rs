//! Native Windows 11 Network, Wi-Fi, Mobile Hotspot ("Compartir Internet"), VPN & Radio Telemetry
//! - Accurately distinguishes Ethernet vs Wi-Fi (real 0-100% signal bars & SSID via wlanapi.dll) vs Offline
//! - Instant Wi-Fi, Bluetooth & Airplane Mode toggles via WinRT Windows.Devices.Radios.Radio (0 UAC prompts!)
//! - Native Windows 11 Mobile Hotspot ("Zona con cobertura inalámbrica móvil") via WinRT
//!   `Windows.Networking.NetworkOperators.NetworkOperatorTetheringManager` (Toggle ON/OFF, SSID, Password, Clients!)
//! - Wi-Fi network scanning / connect / disconnect / forget, Windows VPN profiles, IPv4/Gateway/DNS/MAC/Proxy & Ping!

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows::core::{GUID, HSTRING, PCSTR, PCWSTR};
use windows::Devices::Radios::{Radio, RadioKind, RadioState};
use windows::Networking::Connectivity::NetworkInformation;
use windows::Networking::NetworkOperators::{
    NetworkOperatorTetheringManager, TetheringCapability, TetheringOperationalState,
};
use windows::Win32::Foundation::FreeLibrary;
use windows::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GetBestInterface, GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST,
    GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NetKind {
    Ethernet,
    Wifi,
    #[default]
    Offline,
}

#[derive(Debug, Clone, Default)]
pub struct HotspotInfo {
    pub supported: bool,
    pub is_on: bool,
    pub ssid: String,
    pub passphrase: String,
    pub client_count: u32,
    pub max_clients: u32,
    pub status_reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal_pct: u8,
    pub bars: u8,
    pub secured: bool,
    pub auth: String,
    pub connected: bool,
    pub saved: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VpnProfile {
    pub name: String,
    pub connected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkDetails {
    pub adapter_name: String,
    pub ipv4: String,
    pub gateway: String,
    pub dns_servers: Vec<String>,
    pub mac: String,
    pub link_speed_mbps: u64,
    pub proxy_enabled: bool,
    pub proxy_server: String,
}

#[derive(Debug, Clone, Default)]
pub struct RadioStates {
    pub wifi_available: bool,
    pub wifi_on: bool,
    pub bt_available: bool,
    pub bt_on: bool,
    pub airplane_mode: bool,
    pub ethernet_up: bool,
    pub has_internet: bool,
    pub net_kind: NetKind,
    pub wifi_signal_pct: u8,
    pub wifi_bars: u8, // 0..=4
    pub wifi_ssid: String,
    pub net_label: String,
    pub hotspot: HotspotInfo,
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

    st.airplane_mode = (st.wifi_available || st.bt_available)
        && (!st.wifi_available || !st.wifi_on)
        && (!st.bt_available || !st.bt_on);
    st.hotspot = query_hotspot_info();

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
            if st.airplane_mode {
                "Modo Avión activo".to_string()
            } else if st.wifi_available && !st.wifi_on {
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

/// Toggles Airplane Mode (turns all wireless radios Wi-Fi / Bluetooth / MobileBroadband OFF or back ON).
pub fn set_airplane_mode(enable: bool) -> bool {
    let Ok(op) = Radio::GetRadiosAsync() else { return false };
    let Ok(radios) = op.get() else { return false };
    let Ok(count) = radios.Size() else { return false };
    let target_state = if enable { RadioState::Off } else { RadioState::On };
    let mut touched = false;
    for i in 0..count {
        if let Ok(r) = radios.GetAt(i) {
            let k = r.Kind().unwrap_or(RadioKind::Other);
            if matches!(k, RadioKind::WiFi | RadioKind::Bluetooth | RadioKind::MobileBroadband) {
                if let Ok(set_op) = r.SetStateAsync(target_state) {
                    let _ = set_op.get();
                    touched = true;
                }
            }
        }
    }
    touched
}

// ==================== WINDOWS 11 MOBILE HOTSPOT ("COMPARTIR INTERNET") ====================

pub fn query_hotspot_info() -> HotspotInfo {
    let mut info = HotspotInfo {
        supported: false,
        is_on: false,
        ssid: String::new(),
        passphrase: String::new(),
        client_count: 0,
        max_clients: 8,
        status_reason: "Sin conexión activa para compartir".to_string(),
    };

    let Ok(profile) = NetworkInformation::GetInternetConnectionProfile() else {
        return info;
    };
    let cap = NetworkOperatorTetheringManager::GetTetheringCapabilityFromConnectionProfile(&profile)
        .unwrap_or(TetheringCapability::DisabledByHardwareLimitation);

    match cap {
        TetheringCapability::Enabled => {
            info.supported = true;
            info.status_reason = "Listo para compartir internet".to_string();
        }
        TetheringCapability::DisabledByHardwareLimitation => {
            info.status_reason = "Sin adaptador Wi-Fi compatible para Hotspot".to_string();
            return info;
        }
        TetheringCapability::DisabledByGroupPolicy => {
            info.status_reason = "Deshabilitado por directiva del sistema".to_string();
            return info;
        }
        TetheringCapability::DisabledByOperator | TetheringCapability::DisabledBySku => {
            info.status_reason = "No habilitado en esta edición/red".to_string();
            return info;
        }
        _ => {
            info.status_reason = "Hotspot no disponible en este equipo".to_string();
            return info;
        }
    }

    if let Ok(mgr) = NetworkOperatorTetheringManager::CreateFromConnectionProfile(&profile) {
        let state = mgr
            .TetheringOperationalState()
            .unwrap_or(TetheringOperationalState::Off);
        info.is_on = state == TetheringOperationalState::On;
        info.client_count = mgr.ClientCount().unwrap_or(0);
        info.max_clients = mgr.MaxClientCount().unwrap_or(8);
        if let Ok(cfg) = mgr.GetCurrentAccessPointConfiguration() {
            info.ssid = cfg.Ssid().map(|s| s.to_string()).unwrap_or_default();
            info.passphrase = cfg.Passphrase().map(|s| s.to_string()).unwrap_or_default();
        }
        info.status_reason = if info.is_on {
            format!("Activo · {}/{} dispositivos", info.client_count, info.max_clients)
        } else {
            "Apagado · Listo para activar".to_string()
        };
    }

    info
}

pub fn toggle_hotspot() -> Result<bool, String> {
    let profile = NetworkInformation::GetInternetConnectionProfile()
        .map_err(|_| "No hay conexión a Internet activa para compartir".to_string())?;
    let cap = NetworkOperatorTetheringManager::GetTetheringCapabilityFromConnectionProfile(&profile)
        .unwrap_or(TetheringCapability::DisabledByHardwareLimitation);
    if cap != TetheringCapability::Enabled {
        return Err("Tu PC no tiene adaptador Wi-Fi compatible con Compartir Internet".to_string());
    }
    let mgr = NetworkOperatorTetheringManager::CreateFromConnectionProfile(&profile)
        .map_err(|_| "No se pudo inicializar el gestor de Zona Wi-Fi".to_string())?;
    let is_on = mgr
        .TetheringOperationalState()
        .unwrap_or(TetheringOperationalState::Off)
        == TetheringOperationalState::On;
    if is_on {
        mgr.StopTetheringAsync()
            .map_err(|_| "Error al detener Zona Wi-Fi".to_string())?
            .get()
            .map_err(|_| "No se pudo apagar la Zona Wi-Fi".to_string())?;
        Ok(false)
    } else {
        mgr.StartTetheringAsync()
            .map_err(|_| "Error al iniciar Zona Wi-Fi".to_string())?
            .get()
            .map_err(|_| "No se pudo activar la Zona Wi-Fi (verifica que Wi-Fi esté encendido)".to_string())?;
        Ok(true)
    }
}

pub fn validate_hotspot_credentials(ssid: &str, passphrase: &str) -> Result<(), String> {
    let s = ssid.trim();
    let p = passphrase.trim();
    if s.is_empty() || s.len() > 32 {
        return Err("El nombre de red (SSID) debe tener entre 1 y 32 caracteres".into());
    }
    if p.len() < 8 || p.len() > 63 {
        return Err("La contraseña del Hotspot debe tener entre 8 y 63 caracteres".into());
    }
    Ok(())
}

pub fn configure_hotspot(ssid: &str, passphrase: &str) -> Result<(), String> {
    validate_hotspot_credentials(ssid, passphrase)?;
    let profile = NetworkInformation::GetInternetConnectionProfile()
        .map_err(|_| "Sin perfil de Internet activo".to_string())?;
    let mgr = NetworkOperatorTetheringManager::CreateFromConnectionProfile(&profile)
        .map_err(|_| "Hotspot no disponible en este equipo".to_string())?;
    let cfg = mgr
        .GetCurrentAccessPointConfiguration()
        .map_err(|_| "No se pudo leer la configuración actual del Hotspot".to_string())?;
    cfg.SetSsid(&HSTRING::from(ssid.trim()))
        .map_err(|_| "SSID inválido".to_string())?;
    cfg.SetPassphrase(&HSTRING::from(passphrase.trim()))
        .map_err(|_| "Contraseña inválida".to_string())?;
    mgr.ConfigureAccessPointAsync(&cfg)
        .map_err(|_| "Error aplicando configuración del Hotspot".to_string())?
        .get()
        .map_err(|_| "Windows rechazó la configuración del Hotspot".to_string())?;
    Ok(())
}

// ==================== WI-FI SCANNER & CONNECT / DISCONNECT / FORGET ====================

pub fn parse_saved_wifi_profiles(raw: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for line in raw.lines() {
        let low = line.to_lowercase();
        if low.contains("profile") || low.contains("perfil") {
            if let Some((_, rhs)) = line.split_once(':') {
                let name = rhs.trim();
                if !name.is_empty() {
                    set.insert(name.to_string());
                }
            }
        }
    }
    set
}

pub fn parse_netsh_wifi_networks(
    raw: &str,
    current_ssid: &str,
    saved_profiles: &HashSet<String>,
) -> Vec<WifiNetwork> {
    let mut best: HashMap<String, WifiNetwork> = HashMap::new();
    let mut cur_ssid = String::new();
    let mut cur_auth = String::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        let low = trimmed.to_lowercase();
        if low.starts_with("ssid ") {
            if let Some((_, rhs)) = trimmed.split_once(':') {
                cur_ssid = rhs.trim().to_string();
                cur_auth.clear();
            }
        } else if (low.starts_with("authentication") || low.starts_with("autenticaci"))
            && !cur_ssid.is_empty()
        {
            if let Some((_, rhs)) = trimmed.split_once(':') {
                cur_auth = rhs.trim().to_string();
            }
        } else if (low.starts_with("signal") || low.starts_with("se"))
            && trimmed.contains('%')
            && !cur_ssid.is_empty()
        {
            if let Some((_, rhs)) = trimmed.split_once(':') {
                let pct: u8 = rhs
                    .trim()
                    .trim_end_matches('%')
                    .trim()
                    .parse()
                    .unwrap_or(0)
                    .min(100);
                let secured = !cur_auth.to_lowercase().contains("open")
                    && !cur_auth.to_lowercase().contains("abierta");
                let connected = !current_ssid.is_empty() && cur_ssid.eq_ignore_ascii_case(current_ssid);
                let saved = connected || saved_profiles.contains(&cur_ssid);
                let entry = best.entry(cur_ssid.clone()).or_insert_with(|| WifiNetwork {
                    ssid: cur_ssid.clone(),
                    signal_pct: pct,
                    bars: signal_pct_to_bars(pct),
                    secured,
                    auth: if cur_auth.is_empty() { "WPA2".into() } else { cur_auth.clone() },
                    connected,
                    saved,
                });
                if pct > entry.signal_pct {
                    entry.signal_pct = pct;
                    entry.bars = signal_pct_to_bars(pct);
                }
            }
        }
    }

    let mut out: Vec<WifiNetwork> = best.into_values().collect();
    out.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then_with(|| b.saved.cmp(&a.saved))
            .then_with(|| b.signal_pct.cmp(&a.signal_pct))
            .then_with(|| a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase()))
    });
    out
}

pub fn scan_wifi_networks(current_ssid: &str) -> Vec<WifiNetwork> {
    let saved_raw = Command::new("netsh")
        .args(["wlan", "show", "profiles"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let saved = parse_saved_wifi_profiles(&saved_raw);

    let nets_raw = Command::new("netsh")
        .args(["wlan", "show", "networks", "mode=bssid"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    parse_netsh_wifi_networks(&nets_raw, current_ssid, &saved)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn connect_wifi(ssid: &str, password: Option<&str>) -> Result<String, String> {
    let ssid_clean = ssid.trim();
    if ssid_clean.is_empty() {
        return Err("SSID vacío".into());
    }
    if let Some(pwd) = password {
        if !pwd.is_empty() {
            let xml = format!(
                r#"<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
  <name>{name}</name>
  <SSIDConfig><SSID><name>{name}</name></SSID></SSIDConfig>
  <connectionType>ESS</connectionType>
  <connectionMode>auto</connectionMode>
  <MSM><security><authEncryption><authentication>WPA2PSK</authentication><encryption>AES</encryption><useOneX>false</useOneX></authEncryption><sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>{pwd}</keyMaterial></sharedKey></security></MSM>
</WLANProfile>"#,
                name = xml_escape(ssid_clean),
                pwd = xml_escape(pwd),
            );
            let tmp = std::env::temp_dir().join("aura_wifi_profile.xml");
            let _ = std::fs::write(&tmp, xml);
            let add_ok = Command::new("netsh")
                .args(["wlan", "add", "profile", &format!("filename={}", tmp.to_string_lossy()), "user=current"])
                .creation_flags(CREATE_NO_WINDOW)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            let _ = std::fs::remove_file(&tmp);
            if !add_ok {
                return Err("No se pudo guardar el perfil Wi-Fi".into());
            }
        }
    }
    let out = Command::new("netsh")
        .args(["wlan", "connect", &format!("name={}", ssid_clean)])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|_| "Error ejecutando netsh wlan".to_string())?;
    if out.status.success() {
        Ok(format!("Conectando a '{}'...", ssid_clean))
    } else {
        Err("No se pudo conectar (requiere contraseña o perfil guardado)".into())
    }
}

pub fn disconnect_wifi() -> bool {
    Command::new("netsh")
        .args(["wlan", "disconnect"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn forget_wifi(ssid: &str) -> bool {
    Command::new("netsh")
        .args(["wlan", "delete", "profile", &format!("name={}", ssid)])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ==================== WINDOWS VPN PROFILES ====================

pub fn parse_pbk_profiles(content: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') && t.len() > 2 {
            let name = t[1..t.len() - 1].trim();
            if !name.is_empty() && !names.iter().any(|n: &String| n.eq_ignore_ascii_case(name)) {
                names.push(name.to_string());
            }
        }
    }
    names
}

pub fn list_vpn_profiles() -> Vec<VpnProfile> {
    let mut names: Vec<String> = Vec::new();
    for base_env in ["APPDATA", "ProgramData"] {
        if let Ok(base) = std::env::var(base_env) {
            let pbk = std::path::PathBuf::from(base).join(r"Microsoft\Network\Connections\Pbk\rasphone.pbk");
            if let Ok(txt) = std::fs::read_to_string(pbk) {
                for n in parse_pbk_profiles(&txt) {
                    if !names.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
                        names.push(n);
                    }
                }
            }
        }
    }
    if names.is_empty() {
        return Vec::new();
    }
    let active_raw = Command::new("rasdial")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase())
        .unwrap_or_default();

    names
        .into_iter()
        .map(|name| {
            let connected = active_raw.contains(&name.to_lowercase());
            VpnProfile { name, connected }
        })
        .collect()
}

pub fn toggle_vpn(name: &str, disconnect: bool) -> Result<String, String> {
    let mut cmd = Command::new("rasdial");
    cmd.arg(name);
    if disconnect {
        cmd.arg("/disconnect");
    }
    let ok = cmd
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        Ok(if disconnect {
            format!("VPN '{}' desconectada", name)
        } else {
            format!("VPN '{}' conectada", name)
        })
    } else {
        Err(format!("No se pudo {} VPN '{}' (verifica credenciales en Windows)", if disconnect { "desconectar" } else { "conectar" }, name))
    }
}

// ==================== ADAPTER DETAILS, PROXY & NETWORK DIAGNOSTICS ====================

unsafe fn sockaddr_to_ipv4(sa: *const windows::Win32::Networking::WinSock::SOCKADDR) -> Option<String> {
    if sa.is_null() || (*sa).sa_family != AF_INET {
        return None;
    }
    let sin = &*(sa as *const SOCKADDR_IN);
    let b = sin.sin_addr.S_un.S_addr.to_ne_bytes();
    Some(format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3]))
}

fn query_windows_proxy() -> (bool, String) {
    let mut enabled = false;
    let mut server = String::new();
    unsafe {
        let sub: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings\0"
            .encode_utf16()
            .collect();
        let mut hk = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()), 0, KEY_READ, &mut hk).is_ok() {
            let vn_en: Vec<u16> = "ProxyEnable\0".encode_utf16().collect();
            let mut val = 0u32;
            let mut sz = 4u32;
            if RegQueryValueExW(
                hk,
                PCWSTR(vn_en.as_ptr()),
                None,
                None,
                Some(&mut val as *mut u32 as *mut u8),
                Some(&mut sz),
            )
            .is_ok()
            {
                enabled = val != 0;
            }
            let vn_srv: Vec<u16> = "ProxyServer\0".encode_utf16().collect();
            let mut sbuf = [0u8; 512];
            let mut ssz = sbuf.len() as u32;
            if RegQueryValueExW(
                hk,
                PCWSTR(vn_srv.as_ptr()),
                None,
                None,
                Some(sbuf.as_mut_ptr()),
                Some(&mut ssz),
            )
            .is_ok()
            {
                let u16s: Vec<u16> = sbuf[..ssz as usize]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let end = u16s.iter().position(|&c| c == 0).unwrap_or(u16s.len());
                server = String::from_utf16_lossy(&u16s[..end]).trim().to_string();
            }
            let _ = RegCloseKey(hk);
        }
    }
    (enabled, server)
}

pub fn query_network_details() -> NetworkDetails {
    let mut det = NetworkDetails::default();
    let (pe, ps) = query_windows_proxy();
    det.proxy_enabled = pe;
    det.proxy_server = ps;

    unsafe {
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
            return det;
        }

        let mut fallback: Option<*const IP_ADAPTER_ADDRESSES_LH> = None;
        let mut chosen: Option<*const IP_ADAPTER_ADDRESSES_LH> = None;
        let mut ptr = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !ptr.is_null() {
            let a = &*ptr;
            if a.OperStatus == IfOperStatusUp && (a.IfType == 6 || a.IfType == 71) {
                let desc = a.Description.to_string().unwrap_or_default();
                if !is_virtual_adapter(&desc) && !a.FirstGatewayAddress.is_null() {
                    let if_idx = a.Anonymous1.Anonymous.IfIndex;
                    if has_route && if_idx == best_if_idx {
                        chosen = Some(ptr);
                        break;
                    }
                    if fallback.is_none() {
                        fallback = Some(ptr);
                    }
                }
            }
            ptr = a.Next;
        }

        if let Some(p) = chosen.or(fallback) {
            let a = &*p;
            det.adapter_name = a
                .FriendlyName
                .to_string()
                .or_else(|_| a.Description.to_string())
                .unwrap_or_else(|_| "Adaptador de Red".into());
            det.link_speed_mbps = a.TransmitLinkSpeed / 1_000_000;
            if a.PhysicalAddressLength == 6 {
                let m = &a.PhysicalAddress[..6];
                det.mac = format!(
                    "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                    m[0], m[1], m[2], m[3], m[4], m[5]
                );
            }
            let mut uni = a.FirstUnicastAddress;
            while !uni.is_null() {
                if let Some(ip) = sockaddr_to_ipv4((*uni).Address.lpSockaddr) {
                    det.ipv4 = ip;
                    break;
                }
                uni = (*uni).Next;
            }
            let mut gw = a.FirstGatewayAddress;
            while !gw.is_null() {
                if let Some(ip) = sockaddr_to_ipv4((*gw).Address.lpSockaddr) {
                    det.gateway = ip;
                    break;
                }
                gw = (*gw).Next;
            }
            let mut dns = a.FirstDnsServerAddress;
            while !dns.is_null() {
                if let Some(ip) = sockaddr_to_ipv4((*dns).Address.lpSockaddr) {
                    if !det.dns_servers.contains(&ip) && det.dns_servers.len() < 4 {
                        det.dns_servers.push(ip);
                    }
                }
                dns = (*dns).Next;
            }
        }
    }
    det
}

/// Fast zero-console TCP latency check (to Cloudflare 1.1.1.1:443 with Google 8.8.8.8:53 fallback).
pub fn measure_internet_latency_ms() -> Option<u32> {
    use std::net::{SocketAddr, TcpStream};
    use std::time::{Duration, Instant};
    for addr in ["1.1.1.1:443", "8.8.8.8:53"] {
        if let Ok(sa) = addr.parse::<SocketAddr>() {
            let t0 = Instant::now();
            if TcpStream::connect_timeout(&sa, Duration::from_millis(1500)).is_ok() {
                return Some(t0.elapsed().as_millis().max(1) as u32);
            }
        }
    }
    None
}

pub fn renew_ip_address() -> bool {
    let _ = Command::new("ipconfig")
        .arg("/flushdns")
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    Command::new("ipconfig")
        .arg("/renew")
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_radio_states_and_details() {
        let st = query_radio_states();
        let det = query_network_details();
        println!("radios={:?}\ndetails={:?}", st, det);
        assert!(!st.net_label.is_empty());
    }

    #[test]
    fn test_parse_wifi_spanish_and_english() {
        let sample_es = "\
Nombre de interfaz : Wi-Fi
SSID 1 : CasaFiber_5G
    Tipo de red             : Infraestructura
    Autenticación           : WPA2-Personal
    BSSID 1                 : aa:bb:cc:dd:ee:01
         Señal              : 84%
    BSSID 2                 : aa:bb:cc:dd:ee:02
         Señal              : 62%
SSID 2 : CafeGratis
    Autenticación           : Abierta
    BSSID 1                 : 11:22:33:44:55:66
         Señal              : 45%
";
        let mut saved = HashSet::new();
        saved.insert("CasaFiber_5G".to_string());
        let nets = parse_netsh_wifi_networks(sample_es, "CasaFiber_5G", &saved);
        assert_eq!(nets.len(), 2);
        assert_eq!(nets[0].ssid, "CasaFiber_5G");
        assert_eq!(nets[0].signal_pct, 84);
        assert!(nets[0].connected);
        assert!(nets[0].secured);
        assert_eq!(nets[1].ssid, "CafeGratis");
        assert!(!nets[1].secured);
    }

    #[test]
    fn test_parse_pbk_and_hotspot_validation() {
        let pbk = "[Work VPN]\nType=2\n[Gaming WireGuard]\nType=3\n";
        assert_eq!(parse_pbk_profiles(pbk), vec!["Work VPN", "Gaming WireGuard"]);
        assert!(validate_hotspot_credentials("", "12345678").is_err());
        assert!(validate_hotspot_credentials("AuraPC", "short").is_err());
        assert!(validate_hotspot_credentials("AuraPC", "AuraOS2026!").is_ok());
    }
}
