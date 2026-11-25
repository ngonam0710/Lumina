use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn is_placed(self) -> bool {
        self.right > self.left && self.bottom > self.top && self.left > -9000 && self.top > -9000
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub pid: u32,
    pub abstate: i32,
    pub primary: Option<Rect>,
    pub secondaries: Vec<(isize, Rect)>,
}

impl Marker {
    pub fn new(abstate: i32, pid: u32) -> Self {
        Self {
            pid,
            abstate,
            primary: None,
            secondaries: Vec::new(),
        }
    }

    pub fn secondary_rect(&self, hwnd: isize) -> Option<Rect> {
        self.secondaries.iter().find(|(h, _)| *h == hwnd).map(|(_, r)| *r)
    }

    pub fn serialize(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn parse(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_round_trips() {
        let mut m = Marker::new(2, 1234);
        m.primary = Some(Rect {
            left: 0,
            top: 1032,
            right: 1920,
            bottom: 1080,
        });
        let parsed = Marker::parse(&m.serialize()).unwrap();
        assert_eq!(parsed, m);
    }
}
