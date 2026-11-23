//! AuraOS Native Rust Disk Space Analyzer & Squarified Cushion Treemap Engine
//! - Replicates WinDirStat v2.9's fast Win32 kernel scan (`FindFirstFileExW` + `FindExInfoBasic` +
//!   `FIND_FIRST_EX_LARGE_FETCH`, skipping NTFS reparse points/junctions to avoid loops)
//! - Multi-threaded work-stealing directory scanner with live streaming progress & instant cancellation
//! - Authentic van Wijk & van de Wetering (1999) Squarified Cushion Treemap rendered to a fast BGRA
//!   buffer for 144 FPS Direct2D GPU display with bidirectional hover/click/zoom hit-testing
//! - Curated Drool Dark Mode extension color palette (zero harsh legacy primary colors!)

use crate::renderer::{DecodedAsset, Rect};
use crate::win32_shell::to_wide;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindNextFileW,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FIND_FIRST_EX_LARGE_FETCH,
    WIN32_FIND_DATAW,
};

/// Curated Drool Dark Mode extension palette (RGB tuples designed for dark obsidian UI!)
pub const EXT_PALETTE: [(u8, u8, u8); 14] = [
    (124, 92, 255),  // 0: Electric Violet (Videos / Media)
    (242, 109, 125), // 1: Coral Rose (Executables / Binaries)
    (56, 189, 248),  // 2: Sky Cyan (Archives / Packages / .pak)
    (245, 183, 49),  // 3: Warm Amber (Images / Textures)
    (34, 197, 94),   // 4: Emerald Green (Code / Data / JSON)
    (168, 85, 247),  // 5: Royal Purple (Audio / Music)
    (251, 146, 60),  // 6: Sunset Orange (Disk Images / ISO / VHDX)
    (45, 212, 191),  // 7: Teal Mint (Docs / PDF)
    (99, 102, 241),  // 8: Indigo
    (244, 114, 182), // 9: Pink
    (163, 230, 53),  // 10: Lime
    (96, 165, 250),  // 11: Soft Blue
    (250, 204, 21),  // 12: Gold
    (106, 110, 134), // 13: Neutral Matte Slate (Other / Minor)
];

#[derive(Debug, Clone)]
pub struct DiskNode {
    pub id: usize,
    pub parent: Option<usize>,
    pub name: String,
    pub full_path: String,
    pub is_dir: bool,
    pub size: u64,
    pub file_count: u32,
    pub dir_count: u32,
    pub ext: String,
    pub children: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct ExtStat {
    pub ext: String,
    pub total_bytes: u64,
    pub file_count: u32,
    pub color_idx: usize,
}

#[derive(Debug, Clone)]
pub struct DiskTree {
    pub root_path: PathBuf,
    pub nodes: Vec<DiskNode>,
    pub ext_stats: Vec<ExtStat>,
    pub ext_color_map: HashMap<String, usize>,
    pub elapsed_ms: u64,
}

impl DiskTree {
    pub fn total_bytes(&self) -> u64 {
        self.nodes.first().map(|n| n.size).unwrap_or(0)
    }

    pub fn color_for_ext(&self, ext: &str) -> (u8, u8, u8) {
        let idx = self.ext_color_map.get(ext).copied().unwrap_or(EXT_PALETTE.len() - 1);
        EXT_PALETTE[idx.min(EXT_PALETTE.len() - 1)]
    }

    pub fn color_idx_for_ext(&self, ext: &str) -> usize {
        self.ext_color_map.get(ext).copied().unwrap_or(EXT_PALETTE.len() - 1)
    }

    /// Recomputes ancestor sizes cleanly after deleting a file/folder inside AuraFiles!
    pub fn mark_deleted(&mut self, node_id: usize) {
        if node_id == 0 || node_id >= self.nodes.len() {
            return;
        }
        let removed_size = self.nodes[node_id].size;
        let removed_files = self.nodes[node_id].file_count.max(if self.nodes[node_id].is_dir { 0 } else { 1 });
        let parent_opt = self.nodes[node_id].parent;
        self.nodes[node_id].size = 0;
        self.nodes[node_id].children.clear();
        if let Some(pid) = parent_opt {
            self.nodes[pid].children.retain(|&c| c != node_id);
        }
        let mut cur = parent_opt;
        while let Some(p) = cur {
            self.nodes[p].size = self.nodes[p].size.saturating_sub(removed_size);
            self.nodes[p].file_count = self.nodes[p].file_count.saturating_sub(removed_files);
            cur = self.nodes[p].parent;
        }
        // Re-sort parent's children descending
        if let Some(pid) = parent_opt {
            let mut ch = std::mem::take(&mut self.nodes[pid].children);
            ch.sort_by(|&a, &b| self.nodes[b].size.cmp(&self.nodes[a].size));
            self.nodes[pid].children = ch;
        }
    }
}

pub enum ScanUpdate {
    Progress {
        files: u32,
        dirs: u32,
        bytes: u64,
        current_path: String,
    },
    Finished(DiskTree),
}

pub struct ScanHandle {
    pub rx: Receiver<ScanUpdate>,
    pub cancel: Arc<AtomicBool>,
}

/// Fast Win32 FindFirstFileExW directory scan (identical kernel path to WinDirStat v2.9!)
struct RawDirBatch {
    parent_path: PathBuf,
    subdirs: Vec<(String, PathBuf)>,
    files: Vec<(String, String, u64, String)>, // (name, full_path, size, ext_lower)
}

fn scan_single_dir_win32(dir: &Path) -> RawDirBatch {
    let mut batch = RawDirBatch {
        parent_path: dir.to_path_buf(),
        subdirs: Vec::new(),
        files: Vec::new(),
    };
    let dir_str = dir.to_string_lossy();
    let trimmed = dir_str.trim_end_matches('\\');
    // Extended-length \\?\ prefix handles long Windows paths > 260 chars safely!
    let search_pattern = if trimmed.starts_with(r"\\?\") {
        format!(r"{}\*", trimmed)
    } else if trimmed.len() == 2 && trimmed.ends_with(':') {
        format!(r"\\?\{}\*", trimmed)
    } else {
        format!(r"\\?\{}\*", trimmed)
    };

    let wide = to_wide(&search_pattern);
    let mut fdata = WIN32_FIND_DATAW::default();
    unsafe {
        let Ok(handle): windows::core::Result<HANDLE> = FindFirstFileExW(
            PCWSTR(wide.as_ptr()),
            FindExInfoBasic,
            &mut fdata as *mut WIN32_FIND_DATAW as *mut _,
            FindExSearchNameMatch,
            None,
            FIND_FIRST_EX_LARGE_FETCH,
        ) else {
            return batch;
        };
        if handle.is_invalid() {
            return batch;
        }

        loop {
            let name = crate::win32_shell::from_wide(&fdata.cFileName);
            if name != "." && name != ".." && !name.is_empty() {
                let attrs = fdata.dwFileAttributes;
                let is_reparse = (attrs & FILE_ATTRIBUTE_REPARSE_POINT.0) != 0;
                let is_dir = (attrs & FILE_ATTRIBUTE_DIRECTORY.0) != 0;

                if !is_reparse {
                    let child_path = if trimmed.ends_with('\\') {
                        format!("{}{}", trimmed, name)
                    } else {
                        format!(r"{}\{}", trimmed, name)
                    };
                    if is_dir {
                        batch.subdirs.push((name, PathBuf::from(child_path)));
                    } else {
                        let size = ((fdata.nFileSizeHigh as u64) << 32) | (fdata.nFileSizeLow as u64);
                        let ext = Path::new(&name)
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|s| format!(".{}", s.to_lowercase()))
                            .unwrap_or_else(|| "(sin ext.)".to_string());
                        batch.files.push((name, child_path, size, ext));
                    }
                }
            }
            if FindNextFileW(handle, &mut fdata).is_err() {
                break;
            }
        }
        let _ = FindClose(handle);
    }
    batch
}

/// Launches a 4-thread parallel Win32 FindFirstFileExW scan of any drive or directory!
pub fn start_parallel_scan(root: PathBuf) -> ScanHandle {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_bg = cancel.clone();

    std::thread::spawn(move || {
        let start = Instant::now();
        let files_cnt = Arc::new(AtomicU32::new(0));
        let dirs_cnt = Arc::new(AtomicU32::new(0));
        let bytes_cnt = Arc::new(AtomicU64::new(0));

        // Shared work-stealing directory queue: (node_id, path)
        struct WorkQueue {
            q: VecDeque<(usize, PathBuf)>,
            active_workers: usize,
            done: bool,
        }
        let queue = Arc::new((
            Mutex::new(WorkQueue {
                q: VecDeque::from([(0usize, root.clone())]),
                active_workers: 0,
                done: false,
            }),
            Condvar::new(),
        ));

        let root_name = root
            .file_name()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| root.to_string_lossy().to_string());

        let nodes_arc = Arc::new(Mutex::new(vec![DiskNode {
            id: 0,
            parent: None,
            name: root_name,
            full_path: root.to_string_lossy().to_string(),
            is_dir: true,
            size: 0,
            file_count: 0,
            dir_count: 0,
            ext: String::new(),
            children: Vec::new(),
        }]));

        let ext_map_arc: Arc<Mutex<HashMap<String, (u64, u32)>>> = Arc::new(Mutex::new(HashMap::new()));
        let last_prog = Arc::new(Mutex::new((Instant::now(), String::new())));

        let worker_count = 4usize;
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let q_ref = queue.clone();
            let nodes_ref = nodes_arc.clone();
            let ext_ref = ext_map_arc.clone();
            let cancel_ref = cancel_bg.clone();
            let fc = files_cnt.clone();
            let dc = dirs_cnt.clone();
            let bc = bytes_cnt.clone();
            let lp = last_prog.clone();
            let tx_c = tx.clone();

            handles.push(std::thread::spawn(move || {
                let mut local_ext: HashMap<String, (u64, u32)> = HashMap::new();
                loop {
                    if cancel_ref.load(Ordering::Relaxed) {
                        let (lk, cv) = &*q_ref;
                        let mut g = lk.lock().unwrap();
                        g.done = true;
                        cv.notify_all();
                        break;
                    }

                    let item = {
                        let (lk, cv) = &*q_ref;
                        let mut g = lk.lock().unwrap();
                        loop {
                            if g.done || cancel_ref.load(Ordering::Relaxed) {
                                break None;
                            }
                            if let Some(it) = g.q.pop_front() {
                                g.active_workers += 1;
                                break Some(it);
                            }
                            if g.active_workers == 0 {
                                g.done = true;
                                cv.notify_all();
                                break None;
                            }
                            g = cv.wait(g).unwrap();
                        }
                    };

                    let Some((parent_id, dir_path)) = item else { break };
                    dc.fetch_add(1, Ordering::Relaxed);

                    let batch = scan_single_dir_win32(&dir_path);

                    let mut batch_bytes = 0u64;
                    let batch_files = batch.files.len() as u32;
                    for (_, _, sz, ext) in &batch.files {
                        batch_bytes += *sz;
                        let ent = local_ext.entry(ext.clone()).or_insert((0, 0));
                        ent.0 += *sz;
                        ent.1 += 1;
                    }
                    fc.fetch_add(batch_files, Ordering::Relaxed);
                    bc.fetch_add(batch_bytes, Ordering::Relaxed);

                    // Insert children into shared arena in a single brief lock
                    let mut new_subdir_jobs = Vec::with_capacity(batch.subdirs.len());
                    {
                        let mut nodes = nodes_ref.lock().unwrap();
                        let mut child_ids = Vec::with_capacity(batch.subdirs.len() + batch.files.len());
                        for (name, subpath) in batch.subdirs {
                            let id = nodes.len();
                            nodes.push(DiskNode {
                                id,
                                parent: Some(parent_id),
                                name,
                                full_path: subpath.to_string_lossy().to_string(),
                                is_dir: true,
                                size: 0,
                                file_count: 0,
                                dir_count: 0,
                                ext: String::new(),
                                children: Vec::new(),
                            });
                            child_ids.push(id);
                            new_subdir_jobs.push((id, subpath));
                        }
                        for (name, full_path, size, ext) in batch.files {
                            let id = nodes.len();
                            nodes.push(DiskNode {
                                id,
                                parent: Some(parent_id),
                                name,
                                full_path,
                                is_dir: false,
                                size,
                                file_count: 1,
                                dir_count: 0,
                                ext,
                                children: Vec::new(),
                            });
                            child_ids.push(id);
                        }
                        nodes[parent_id].children = child_ids;
                    }

                    // Push discovered subdirs and decrement active worker count
                    {
                        let (lk, cv) = &*q_ref;
                        let mut g = lk.lock().unwrap();
                        for job in new_subdir_jobs {
                            g.q.push_back(job);
                        }
                        g.active_workers -= 1;
                        if g.q.is_empty() && g.active_workers == 0 {
                            g.done = true;
                        }
                        cv.notify_all();
                    }

                    // Throttle progress notifications to ~16 Hz so UI stays butter-smooth
                    if let Ok(mut pg) = lp.try_lock() {
                        if pg.0.elapsed().as_millis() >= 65 {
                            pg.0 = Instant::now();
                            let _ = tx_c.send(ScanUpdate::Progress {
                                files: fc.load(Ordering::Relaxed),
                                dirs: dc.load(Ordering::Relaxed),
                                bytes: bc.load(Ordering::Relaxed),
                                current_path: dir_path.to_string_lossy().to_string(),
                            });
                        }
                    }
                }

                // Merge thread-local extension stats once on completion
                if let Ok(mut global_ext) = ext_ref.lock() {
                    for (k, (sz, cnt)) in local_ext {
                        let e = global_ext.entry(k).or_insert((0, 0));
                        e.0 += sz;
                        e.1 += cnt;
                    }
                }
            }));
        }

        for h in handles {
            let _ = h.join();
        }
        if cancel_bg.load(Ordering::Relaxed) {
            return;
        }

        // Bottom-up exact aggregation (children always have id > parent_id!)
        let mut nodes = Arc::try_unwrap(nodes_arc).unwrap().into_inner().unwrap();
        for i in (0..nodes.len()).rev() {
            if nodes[i].is_dir {
                let mut total_sz = 0u64;
                let mut total_f = 0u32;
                let mut total_d = 0u32;
                let mut ch = std::mem::take(&mut nodes[i].children);
                for &cid in &ch {
                    total_sz += nodes[cid].size;
                    total_f += nodes[cid].file_count;
                    if nodes[cid].is_dir {
                        total_d += 1 + nodes[cid].dir_count;
                    }
                }
                ch.sort_by(|&a, &b| nodes[b].size.cmp(&nodes[a].size));
                nodes[i].size = total_sz;
                nodes[i].file_count = total_f;
                nodes[i].dir_count = total_d;
                nodes[i].children = ch;
            }
        }

        // Build sorted extension statistics & curated Drool color map
        let raw_ext = Arc::try_unwrap(ext_map_arc).unwrap().into_inner().unwrap();
        let mut ext_vec: Vec<(String, u64, u32)> = raw_ext
            .into_iter()
            .filter(|(_, (sz, _))| *sz > 0)
            .map(|(k, (sz, cnt))| (k, sz, cnt))
            .collect();
        ext_vec.sort_by(|a, b| b.1.cmp(&a.1));

        let mut ext_stats = Vec::with_capacity(ext_vec.len().min(24));
        let mut ext_color_map = HashMap::new();
        let max_distinct = EXT_PALETTE.len() - 1;
        for (i, (ext, total_bytes, file_count)) in ext_vec.into_iter().enumerate() {
            let color_idx = if i < max_distinct { i } else { max_distinct };
            ext_color_map.insert(ext.clone(), color_idx);
            if i < 24 {
                ext_stats.push(ExtStat { ext, total_bytes, file_count, color_idx });
            }
        }

        let tree = DiskTree {
            root_path: root,
            nodes,
            ext_stats,
            ext_color_map,
            elapsed_ms: start.elapsed().as_millis() as u64,
        };
        let _ = tx.send(ScanUpdate::Finished(tree));
    });

    ScanHandle { rx, cancel }
}

// ==================== SQUARIFIED CUSHION TREEMAP ENGINE (van Wijk 1999) ====================

#[derive(Debug, Clone)]
pub struct TreemapCell {
    pub node_id: usize,
    pub rect: Rect,
    pub ext: String,
}

pub struct TreemapRender {
    pub asset: DecodedAsset,
    pub cells: Vec<TreemapCell>,
    /// Exact pixel bounding box of EVERY folder & file node on the treemap (authentic WinDirStat folder outline!)
    pub node_rects: HashMap<usize, Rect>,
    pub zoom_node_id: usize,
    pub width: u32,
    pub height: u32,
}

/// Parabolic Cushion Surface coefficients (s0*x^2 + s1*x + s2*y^2 + s3*y)
#[derive(Clone, Copy, Default)]
struct CushionSurface {
    ax2: f32,
    ax1: f32,
    ay2: f32,
    ay1: f32,
}

impl CushionSurface {
    fn add_ridge(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, h: f32) {
        let dx = (x2 - x1).max(1.0);
        let dy = (y2 - y1).max(1.0);
        let c4h_x = 4.0 * h / dx;
        self.ax2 += c4h_x;
        self.ax1 += c4h_x * (x1 + x2);
        let c4h_y = 4.0 * h / dy;
        self.ay2 += c4h_y;
        self.ay1 += c4h_y * (y1 + y2);
    }
}

/// Renders an authentic WinDirStat Cushion Treemap into a fast GPU-ready premultiplied BGRA buffer!
pub fn render_cushion_treemap(
    tree: &DiskTree,
    zoom_node_id: usize,
    width: u32,
    height: u32,
    highlight_ext: Option<&str>,
) -> TreemapRender {
    let w = width.max(64);
    let h = height.max(64);
    let mut bgra = vec![0u8; (w * h * 4) as usize];

    // Fill crisp matte obsidian canvas background (#0F0F16)
    for px in bgra.chunks_exact_mut(4) {
        px[0] = 22; // B
        px[1] = 15; // G
        px[2] = 15; // R
        px[3] = 255; // A
    }

    let mut cells = Vec::with_capacity(2048);
    let mut node_rects: HashMap<usize, Rect> = HashMap::with_capacity(2048);
    if zoom_node_id < tree.nodes.len() && tree.nodes[zoom_node_id].size > 0 {
        let root_surf = CushionSurface::default();
        layout_node_squarified(
            tree,
            zoom_node_id,
            Rect::new(1.0, 1.0, (w - 2) as f32, (h - 2) as f32),
            root_surf,
            0.52,
            0,
            highlight_ext,
            w,
            h,
            &mut bgra,
            &mut cells,
            &mut node_rects,
        );
    }

    TreemapRender {
        asset: DecodedAsset { width: w, height: h, bgra },
        cells,
        node_rects,
        zoom_node_id,
        width: w,
        height: h,
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_node_squarified(
    tree: &DiskTree,
    node_id: usize,
    r: Rect,
    mut surf: CushionSurface,
    ridge_h: f32,
    depth: u32,
    highlight_ext: Option<&str>,
    buf_w: u32,
    buf_h: u32,
    bgra: &mut [u8],
    cells: &mut Vec<TreemapCell>,
    node_rects: &mut HashMap<usize, Rect>,
) {
    if r.w < 2.0 || r.h < 2.0 {
        return;
    }
    let node = &tree.nodes[node_id];
    if node.size == 0 {
        return;
    }

    // Record exact bounding rectangle for EVERY visible folder & file node (WinDirStat folder highlight!)
    node_rects.insert(node_id, r);

    // Add parabolic cushion ridge for this directory/file level!
    if depth > 0 {
        surf.add_ridge(r.x, r.y, r.x + r.w, r.y + r.h, ridge_h);
    }

    // Leaf file OR tiny directory too small to subdivide cleanly -> shade & record hit cell!
    if !node.is_dir || node.children.is_empty() || (r.w < 4.5 && r.h < 4.5) || depth >= 14 {
        let mut rgb = if node.is_dir {
            (78, 82, 108)
        } else {
            tree.color_for_ext(&node.ext)
        };
        if let Some(hex) = highlight_ext {
            if node.is_dir || !node.ext.eq_ignore_ascii_case(hex) {
                rgb = ((rgb.0 as f32 * 0.25) as u8, (rgb.1 as f32 * 0.25) as u8, (rgb.2 as f32 * 0.25) as u8);
            }
        }
        shade_cushion_rect(r, surf, rgb, buf_w, buf_h, bgra);
        if r.w >= 3.0 && r.h >= 3.0 && cells.len() < 12000 {
            cells.push(TreemapCell { node_id, rect: r, ext: node.ext.clone() });
        }
        return;
    }

    // Filter non-zero children (already sorted descending by size!)
    let children: Vec<usize> = node
        .children
        .iter()
        .copied()
        .filter(|&cid| tree.nodes[cid].size > 0)
        .collect();
    if children.is_empty() {
        return;
    }

    // Squarified Treemap layout (Bruls, Huizing, van Wijk 2000)
    let mut rem_rect = r;
    let mut rem_size = children.iter().map(|&c| tree.nodes[c].size as f64).sum::<f64>();
    let mut idx = 0usize;

    while idx < children.len() && rem_rect.w >= 1.5 && rem_rect.h >= 1.5 && rem_size > 0.0 {
        let horiz = rem_rect.w >= rem_rect.h; // lay out vertical strip along shorter side (rem_rect.h)
        let side = if horiz { rem_rect.h as f64 } else { rem_rect.w as f64 };
        let total_area = (rem_rect.w as f64) * (rem_rect.h as f64);
        let scale = total_area / rem_size;

        let mut row_end = idx + 1;
        let mut row_area = (tree.nodes[children[idx]].size as f64) * scale;
        let mut cur_worst = worst_aspect(row_area, row_area, row_area, side);

        while row_end < children.len() {
            let next_a = (tree.nodes[children[row_end]].size as f64) * scale;
            let new_sum = row_area + next_a;
            let max_a = (tree.nodes[children[idx]].size as f64) * scale; // descending sorted!
            let min_a = next_a.max(0.0001);
            let new_worst = worst_aspect(new_sum, max_a, min_a, side);
            if new_worst <= cur_worst {
                row_area = new_sum;
                cur_worst = new_worst;
                row_end += 1;
            } else {
                break;
            }
        }

        let strip_thick = (row_area / side.max(1.0)) as f32;
        let mut cursor = if horiz { rem_rect.y } else { rem_rect.x };
        let row_bytes: f64 = children[idx..row_end].iter().map(|&c| tree.nodes[c].size as f64).sum();

        for &cid in &children[idx..row_end] {
            let frac = (tree.nodes[cid].size as f64 / row_bytes.max(1.0)) as f32;
            let span = frac * (side as f32);
            let cr = if horiz {
                Rect::new(rem_rect.x, cursor, strip_thick.min(rem_rect.w), span.min(rem_rect.y + rem_rect.h - cursor))
            } else {
                Rect::new(cursor, rem_rect.y, span.min(rem_rect.x + rem_rect.w - cursor), strip_thick.min(rem_rect.h))
            };
            cursor += span;

            layout_node_squarified(
                tree,
                cid,
                cr,
                surf,
                ridge_h * 0.78,
                depth + 1,
                highlight_ext,
                buf_w,
                buf_h,
                bgra,
                cells,
                node_rects,
            );
        }

        if horiz {
            rem_rect.x += strip_thick;
            rem_rect.w = (rem_rect.w - strip_thick).max(0.0);
        } else {
            rem_rect.y += strip_thick;
            rem_rect.h = (rem_rect.h - strip_thick).max(0.0);
        }
        rem_size = (rem_size - row_bytes).max(0.0);
        idx = row_end;
    }
}

fn worst_aspect(sum: f64, max_a: f64, min_a: f64, side: f64) -> f64 {
    if sum <= 0.0 || min_a <= 0.0 || side <= 0.0 {
        return f64::MAX;
    }
    let s2 = side * side;
    let sum2 = sum * sum;
    let r1 = (s2 * max_a) / sum2;
    let r2 = sum2 / (s2 * min_a);
    r1.max(r2)
}

/// Shades a single leaf cushion rectangle using exact van Wijk parabolic normal lighting!
fn shade_cushion_rect(
    r: Rect,
    s: CushionSurface,
    rgb: (u8, u8, u8),
    buf_w: u32,
    buf_h: u32,
    bgra: &mut [u8],
) {
    let x0 = (r.x.floor() as i32).clamp(0, buf_w as i32 - 1) as u32;
    let y0 = (r.y.floor() as i32).clamp(0, buf_h as i32 - 1) as u32;
    let x1 = ((r.x + r.w).floor() as i32).clamp(x0 as i32, buf_w as i32) as u32;
    let y1 = ((r.y + r.h).floor() as i32).clamp(y0 as i32, buf_h as i32) as u32;

    // Normalized soft upper-left directional light (SequoiaView / WinDirStat lighting model)
    const LX: f32 = -0.35;
    const LY: f32 = -0.35;
    const LZ: f32 = 0.87;
    const IA: f32 = 0.24;
    const IS: f32 = 0.82;

    let rf = rgb.0 as f32;
    let gf = rgb.1 as f32;
    let bf = rgb.2 as f32;

    for py in y0..y1 {
        let ny = -(2.0 * s.ay2 * (py as f32 + 0.5) - s.ay1);
        let row_off = (py * buf_w) as usize;
        let is_edge_y = py == y0;
        for px in x0..x1 {
            let is_edge = is_edge_y || px == x0;
            let nx = -(2.0 * s.ax2 * (px as f32 + 0.5) - s.ax1);
            let inv_len = 1.0 / (nx * nx + ny * ny + 1.0).sqrt();
            let cosa = ((nx * LX + ny * LY + LZ) * inv_len).max(0.0);
            let mut intensity = (IA + IS * cosa).clamp(0.14, 1.08);
            if is_edge && (x1 - x0) > 3 && (y1 - y0) > 3 {
                intensity *= 0.62; // subtle dark obsidian hairline between cushion blocks
            }
            let idx = (row_off + px as usize) * 4;
            bgra[idx] = (bf * intensity).clamp(0.0, 255.0) as u8;
            bgra[idx + 1] = (gf * intensity).clamp(0.0, 255.0) as u8;
            bgra[idx + 2] = (rf * intensity).clamp(0.0, 255.0) as u8;
            bgra[idx + 3] = 255;
        }
    }
}
