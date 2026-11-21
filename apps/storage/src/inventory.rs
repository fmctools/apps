//! File lengths are logical bytes, never inferred allocation-chain lengths.
use cmf_wp2_app::storage::Storage;
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum Category {
    App(u8),
    Loader,
    Face,
    #[default]
    Other,
}
#[derive(Clone, Copy)]
pub struct File {
    pub path: [u8; 96],
    pub size: u32,
    pub category: Category,
}
pub const EMPTY_FILE: File = File {
    path: [0; 96],
    size: 0,
    category: Category::Other,
};
impl File {
    pub fn path(&self) -> &str {
        let n = self.path.iter().position(|&b| b == 0).unwrap_or(96);
        core::str::from_utf8(&self.path[..n]).unwrap_or("Invalid filename")
    }
    pub fn name(&self) -> &str {
        self.path().rsplit('/').next().unwrap_or(self.path())
    }
}
#[derive(Clone, Copy)]
pub struct App {
    pub name: [u8; 24],
    pub version: u32,
    pub valid: bool,
    pub icon: bool,
}
impl App {
    pub fn name(&self) -> &str {
        let n = self.name.iter().position(|&b| b == 0).unwrap_or(24);
        core::str::from_utf8(&self.name[..n]).unwrap_or("Unknown app")
    }
}
const EMPTY_APP: App = App {
    name: [0; 24],
    version: 0,
    valid: false,
    icon: false,
};
#[derive(Clone, Copy)]
pub struct Inventory {
    pub files: [File; 96],
    pub count: usize,
    pub apps: [App; 16],
    pub ready: bool,
    pub partial: bool,
    pub error: i32,
}
impl Inventory {
    pub const fn new() -> Self {
        Self {
            files: [EMPTY_FILE; 96],
            count: 0,
            apps: [EMPTY_APP; 16],
            ready: false,
            partial: false,
            error: 0,
        }
    }
    #[inline(never)]
    pub fn reset(&mut self) {
        self.count = 0;
        self.ready = false;
        self.partial = false;
        self.error = 0;
        for a in &mut self.apps {
            a.valid = false;
        }
    }
    #[inline(never)]
    pub fn copy_from(&mut self, other: &Self) {
        unsafe {
            core::ptr::copy_nonoverlapping(other, self, 1);
        }
    }
    pub fn load_file(&mut self, s: &Storage<'_>, i: usize) -> Result<(), i32> {
        let entry = s.file(i as u16)?;
        let mut f = File {
            path: entry.path,
            size: entry.size,
            category: Category::Other,
        };
        f.category = classify(f.path());
        self.files[i] = f;
        self.count = i + 1;
        Ok(())
    }
    pub fn load_apps(&mut self, s: &Storage<'_>) {
        let slots = match s.capabilities() {
            Ok(c) => {
                if c.app_slots > 16 {
                    self.partial = true;
                }
                c.app_slots.min(16)
            }
            Err(e) => {
                self.error = e;
                self.partial = true;
                return;
            }
        };
        for i in 0..slots {
            let mut a = EMPTY_APP;
            match s.installed_app(i) {
                Ok(Some(info)) => {
                    a.valid = true;
                    a.icon = info.has_icon;
                    a.version = info.version;
                    a.name = info.name;
                }
                Ok(None) => {}
                Err(e) => {
                    self.error = e;
                    self.partial = true;
                }
            }
            self.apps[i as usize] = a;
        }
    }
    pub fn sum(&self, cat: Category) -> u32 {
        self.files[..self.count]
            .iter()
            .filter(|f| same_group(f.category, cat))
            .fold(0u32, |a, f| a.saturating_add(f.size))
    }
    pub fn app_total(&self, slot: u8) -> u32 {
        self.files[..self.count]
            .iter()
            .filter(|f| f.category == Category::App(slot))
            .fold(0u32, |a, f| a.saturating_add(f.size))
    }
    pub fn all_bytes(&self) -> u32 {
        self.files[..self.count]
            .iter()
            .fold(0u32, |a, f| a.saturating_add(f.size))
    }
}
pub fn same_group(a: Category, b: Category) -> bool {
    match (a, b) {
        (Category::App(_), Category::App(_)) => true,
        _ => a == b,
    }
}
pub fn classify(path: &str) -> Category {
    if let Some((dir, name)) = path.rsplit_once('/') {
        if dir.eq_ignore_ascii_case("/NAND:/WP2APPS") {
            if name.eq_ignore_ascii_case("L.PKG") {
                return Category::Loader;
            }
            let b = name.as_bytes();
            let slot = match b.first().copied() {
                Some(c @ b'0'..=b'9') => Some(c - b'0'),
                Some(c @ b'A'..=b'F') => Some(c - b'A' + 10),
                Some(c @ b'a'..=b'f') => Some(c - b'a' + 10),
                _ => None,
            };
            if b.len() == 5
                && b[1] == b'.'
                && slot.is_some()
                && [b"PKG", b"DAT", b"WGT", b"PSH"]
                    .iter()
                    .any(|e| b[2..].eq_ignore_ascii_case(*e))
            {
                return Category::App(slot.unwrap());
            }
        }
    }
    if path
        .rsplit('.')
        .next()
        .is_some_and(|e| e.eq_ignore_ascii_case("wfc"))
    {
        Category::Face
    } else {
        Category::Other
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_only_sdk_owned_paths() {
        assert_eq!(classify("/NAND:/wp2apps/0.pkg"), Category::App(0));
        assert_eq!(classify("/NAND:/wp2apps/7.DAT"), Category::App(7));
        assert_eq!(classify("/NAND:/0.pkg"), Category::Other);
        assert_eq!(classify("/NAND:/wp2apps/8.pkg"), Category::App(8));
        assert_eq!(classify("/NAND:/wp2apps/F.psh"), Category::App(15));
        assert_eq!(classify("/NAND:/wp2apps/G.pkg"), Category::Other);
        assert_eq!(classify("/NAND:/wp2apps/0.tmp"), Category::Other);
        assert_eq!(classify("/NAND:/wp2apps/L.PKG"), Category::Loader);
    }
    #[test]
    fn face_extension_is_a_file_category() {
        assert_eq!(classify("/NAND:/foo.WFC"), Category::Face);
        assert_eq!(classify("/NAND:/a.wfc.tmp"), Category::Other);
    }
}

/// Logical file groups plus the remaining measured allocation. Never imply per-file cluster-chain accounting.
pub fn breakdown(fs: &cmf_wp2_app::storage::Filesystem, inv: &Inventory) -> Option<[u32; 4]> {
    if !inv.ready || inv.partial {
        return None;
    }
    let used = fs.total.checked_sub(fs.free)?;
    if inv.all_bytes() > used {
        return None;
    }
    let apps = inv.sum(Category::App(0));
    let faces = inv.sum(Category::Face);
    let loader = inv.sum(Category::Loader);
    Some([
        apps,
        loader,
        faces,
        used.checked_sub(apps)?
            .checked_sub(faces)?
            .checked_sub(loader)?,
    ])
}

#[cfg(test)]
mod breakdown_tests {
    use super::*;
    #[test]
    fn allocation_remainder_is_explicit_and_inconsistent_scans_are_hidden() {
        let mut inv = Inventory::new();
        inv.ready = true;
        inv.count = 4;
        for (i, (category, size)) in [
            (Category::App(0), 20),
            (Category::Face, 10),
            (Category::Loader, 30),
            (Category::Other, 5),
        ]
        .into_iter()
        .enumerate()
        {
            inv.files[i].category = category;
            inv.files[i].size = size;
        }
        let mut fs = cmf_wp2_app::storage::Filesystem {
            total: 200,
            free: 100,
            ..Default::default()
        };
        assert_eq!(breakdown(&fs, &inv), Some([20, 30, 10, 40]));
        inv.partial = true;
        assert_eq!(breakdown(&fs, &inv), None);
        inv.partial = false;
        fs.free = 190;
        assert_eq!(breakdown(&fs, &inv), None);
    }
}
