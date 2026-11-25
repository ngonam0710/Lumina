#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WinInfo {
    pub hwnd: isize,
    pub path: String,
    pub title: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockItem {
    pub key: String,
    pub path: String,
    pub name: String,
    pub pinned: bool,
    pub hwnds: Vec<isize>,
}

impl DockItem {
    pub fn running(&self) -> bool {
        !self.hwnds.is_empty()
    }
}

pub fn norm_key(path: &str) -> String {
    path.replace('/', "\\").trim().to_ascii_lowercase()
}

pub fn merge_items(pins: &[String], wins: &[WinInfo]) -> Vec<DockItem> {
    let mut out: Vec<DockItem> = Vec::new();
    for p in pins {
        let k = norm_key(p);
        if k.is_empty() || out.iter().any(|i| i.key == k) {
            continue;
        }
        let name = std::path::Path::new(p)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| p.clone());
        out.push(DockItem {
            key: k,
            path: p.clone(),
            name,
            pinned: true,
            hwnds: Vec::new(),
        });
    }
    for w in wins {
        let k = norm_key(&w.path);
        if let Some(item) = out.iter_mut().find(|i| i.key == k) {
            item.hwnds.push(w.hwnd);
        } else {
            out.push(DockItem {
                key: k,
                path: w.path.clone(),
                name: w.name.clone(),
                pinned: false,
                hwnds: vec![w.hwnd],
            });
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Item,
    Separator,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub x: f32,
    pub w: f32,
    pub scale: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub cells: Vec<Cell>,
    pub total: f32,
}

pub fn compute_layout(slots: &[SlotKind], cursor_x: Option<f32>, icon: f32, gap: f32, max_scale: f32, area_w: f32) -> Layout {
    let sep = (icon * 0.28).max(8.0);
    let reach = icon * 2.3;
    let mut base_x = Vec::with_capacity(slots.len());
    let mut cur = 0.0f32;
    for (i, s) in slots.iter().enumerate() {
        let w = if *s == SlotKind::Separator { sep } else { icon };
        base_x.push(cur + w / 2.0);
        cur += w;
        if i + 1 < slots.len() {
            cur += gap;
        }
    }
    let rest_total = cur;
    let rest_left = (area_w - rest_total) / 2.0;

    let mut scales = Vec::with_capacity(slots.len());
    for (i, s) in slots.iter().enumerate() {
        if *s == SlotKind::Separator {
            scales.push(1.0);
            continue;
        }
        let sc = match cursor_x {
            Some(cx) => {
                let d = (cx - (rest_left + base_x[i])).abs();
                if d >= reach {
                    1.0
                } else {
                    let t = 1.0 - d / reach;
                    let smooth = t * t * (3.0 - 2.0 * t);
                    1.0 + (max_scale - 1.0) * smooth
                }
            }
            None => 1.0,
        };
        scales.push(sc);
    }

    let total: f32 = slots
        .iter()
        .enumerate()
        .map(|(i, s)| if *s == SlotKind::Separator { sep } else { icon * scales[i] })
        .sum::<f32>()
        + gap * slots.len().saturating_sub(1) as f32;

    let mut x = (area_w - total) / 2.0;
    let mut cells = Vec::with_capacity(slots.len());
    for (i, s) in slots.iter().enumerate() {
        let w = if *s == SlotKind::Separator { sep } else { icon * scales[i] };
        cells.push(Cell { x, w, scale: scales[i] });
        x += w + gap;
    }
    Layout { cells, total }
}

pub fn hit_cell(layout: &Layout, x: f32) -> Option<usize> {
    layout.cells.iter().position(|c| x >= c.x && x < c.x + c.w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dock_layout_magnifies_center_and_hits_correctly() {
        let slots = [SlotKind::Item, SlotKind::Item, SlotKind::Separator, SlotKind::Item];
        let rest = compute_layout(&slots, None, 32.0, 8.0, 1.4, 800.0);
        assert!(rest.cells.iter().all(|c| (c.scale - 1.0).abs() < 1e-4));
        let cx = rest.cells[1].x + rest.cells[1].w / 2.0;
        let zoomed = compute_layout(&slots, Some(cx), 32.0, 8.0, 1.4, 800.0);
        assert!(zoomed.cells[1].scale > 1.35);
        assert_eq!(hit_cell(&zoomed, cx), Some(1));
    }
}
