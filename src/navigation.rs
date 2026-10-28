#[derive(Clone, Debug)]
pub struct History<T> {
    back: Vec<T>,
    forward: Vec<T>,
}
impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            back: vec![],
            forward: vec![],
        }
    }
}
impl<T> History<T> {
    pub fn remap(&mut self, mut map: impl FnMut(T) -> Option<T>) {
        self.back = std::mem::take(&mut self.back)
            .into_iter()
            .filter_map(&mut map)
            .collect();
        self.forward = std::mem::take(&mut self.forward)
            .into_iter()
            .filter_map(map)
            .collect();
    }
    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }
    pub fn clear(&mut self) {
        self.back.clear();
        self.forward.clear();
    }
    pub fn visit(&mut self, previous: T) {
        if self.back.len() >= 128 {
            self.back.remove(0);
        }
        self.back.push(previous);
        self.forward.clear();
    }
    pub fn back(&mut self, current: T) -> Option<T> {
        let target = self.back.pop()?;
        self.forward.push(current);
        Some(target)
    }
    pub fn forward(&mut self, current: T) -> Option<T> {
        let target = self.forward.pop()?;
        self.back.push(current);
        Some(target)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_roundtrip_and_branch() {
        let mut h = History::default();
        h.visit("home");
        h.visit("cache");
        assert_eq!(h.back("yay"), Some("cache"));
        assert_eq!(h.back("cache"), Some("home"));
        assert_eq!(h.back("home"), None);
        assert_eq!(h.forward("home"), Some("cache"));
        h.visit("cache");
        assert!(!h.can_forward());
        assert_eq!(h.back("games"), Some("cache"));
        h.clear();
        assert!(!h.can_back() && !h.can_forward());
    }
}
