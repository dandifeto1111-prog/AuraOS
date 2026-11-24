//! 100% Non-Blocking MTA Background Telemetry Worker for AuraShell v4.1
//! - Moves ALL heavy WinRT / Core Audio COM / Process Snapshot / Network / Radio polling
//!   completely OFF the UI thread!
//! - Zero STA COM deadlocks, zero Control Center / Overview open lag even during heavy games or Spotify!

use crate::audio;
use crate::models::{AudioAppSession, AudioDevice, BackgroundApp, DevPort, NowPlaying};
use crate::radios::{self, RadioStates};
use crate::system;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

#[derive(Debug, Clone, Copy)]
pub enum TelemetryCmd {
    RefreshControlCenter,
    RefreshOverview,
    RefreshNetwork,
    SetControlCenterOpen(bool),
}

#[derive(Debug, Clone)]
pub enum TelemetryUpdate {
    FastMedia {
        master_vol: i32,
        master_muted: bool,
        now_playing: NowPlaying,
    },
    Network(RadioStates),
    ControlCenterData {
        output_devices: Vec<AudioDevice>,
        mic_devices: Vec<AudioDevice>,
        app_sessions: Vec<AudioAppSession>,
        tray_apps: Vec<BackgroundApp>,
    },
    OverviewData {
        dev_ports: Vec<DevPort>,
        tray_apps: Vec<BackgroundApp>,
    },
}

pub struct TelemetryWorker {
    pub tx: Sender<TelemetryCmd>,
    pub rx: Receiver<TelemetryUpdate>,
}

impl TelemetryWorker {
    pub fn spawn() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<TelemetryCmd>();
        let (upd_tx, upd_rx) = mpsc::channel::<TelemetryUpdate>();

        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }

            // Warm initial cache immediately after launch so first Control Center open is 0ms!
            let (mv, mm) = audio::get_master_volume();
            let np = audio::get_now_playing();
            let _ = upd_tx.send(TelemetryUpdate::FastMedia {
                master_vol: mv,
                master_muted: mm,
                now_playing: np,
            });
            let _ = upd_tx.send(TelemetryUpdate::Network(radios::query_radio_states()));
            let _ = upd_tx.send(TelemetryUpdate::ControlCenterData {
                output_devices: audio::get_audio_devices(false),
                mic_devices: audio::get_audio_devices(true),
                app_sessions: audio::get_app_audio_sessions(),
                tray_apps: system::scan_background_tray_apps(),
            });

            let mut last_fast = Instant::now();
            let mut last_net = Instant::now();
            let mut last_cc = Instant::now();
            let mut cc_is_open = false;

            loop {
                // Coalesce on-demand requests cleanly without busy spinning
                match cmd_rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(first_cmd) => {
                        let mut want_cc = matches!(first_cmd, TelemetryCmd::RefreshControlCenter);
                        let mut want_ov = matches!(first_cmd, TelemetryCmd::RefreshOverview);
                        let mut want_net = matches!(first_cmd, TelemetryCmd::RefreshNetwork);
                        if let TelemetryCmd::SetControlCenterOpen(v) = first_cmd {
                            cc_is_open = v;
                        }
                        while let Ok(more) = cmd_rx.try_recv() {
                            match more {
                                TelemetryCmd::RefreshControlCenter => want_cc = true,
                                TelemetryCmd::RefreshOverview => want_ov = true,
                                TelemetryCmd::RefreshNetwork => want_net = true,
                                TelemetryCmd::SetControlCenterOpen(v) => cc_is_open = v,
                            }
                        }
                        if want_net {
                            let _ = upd_tx.send(TelemetryUpdate::Network(radios::query_radio_states()));
                            last_net = Instant::now();
                        }
                        if want_cc {
                            let (mv, mm) = audio::get_master_volume();
                            let np = audio::get_now_playing();
                            let _ = upd_tx.send(TelemetryUpdate::FastMedia {
                                master_vol: mv,
                                master_muted: mm,
                                now_playing: np,
                            });
                            let _ = upd_tx.send(TelemetryUpdate::ControlCenterData {
                                output_devices: audio::get_audio_devices(false),
                                mic_devices: audio::get_audio_devices(true),
                                app_sessions: audio::get_app_audio_sessions(),
                                tray_apps: system::scan_background_tray_apps(),
                            });
                            let _ = upd_tx.send(TelemetryUpdate::Network(radios::query_radio_states()));
                            last_fast = Instant::now();
                            last_cc = Instant::now();
                            last_net = Instant::now();
                        }
                        if want_ov {
                            let _ = upd_tx.send(TelemetryUpdate::OverviewData {
                                dev_ports: system::scan_dev_ports(),
                                tray_apps: system::scan_background_tray_apps(),
                            });
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }

                // Lightweight cached-SMTC + master volume check every 1.5s (0 COM manager allocations!)
                if last_fast.elapsed() >= Duration::from_millis(1500) {
                    let (mv, mm) = audio::get_master_volume();
                    let np = audio::get_now_playing();
                    let _ = upd_tx.send(TelemetryUpdate::FastMedia {
                        master_vol: mv,
                        master_muted: mm,
                        now_playing: np,
                    });
                    last_fast = Instant::now();
                }

                // Network radio WinRT check: 20s idle (refreshed instantly when opening Control Center!)
                if last_net.elapsed() >= Duration::from_secs(20) {
                    let _ = upd_tx.send(TelemetryUpdate::Network(radios::query_radio_states()));
                    last_net = Instant::now();
                }

                // ZERO background CoreAudio/Toolhelp32 churn while Control Center is closed!
                // Only refresh heavy audio devices + per-app mixer + tray apps when Control Center is open.
                if cc_is_open && last_cc.elapsed() >= Duration::from_secs(4) {
                    let _ = upd_tx.send(TelemetryUpdate::ControlCenterData {
                        output_devices: audio::get_audio_devices(false),
                        mic_devices: audio::get_audio_devices(true),
                        app_sessions: audio::get_app_audio_sessions(),
                        tray_apps: system::scan_background_tray_apps(),
                    });
                    last_cc = Instant::now();
                }
            }
        });

        Self { tx: cmd_tx, rx: upd_rx }
    }
}
