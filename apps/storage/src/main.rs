#![cfg_attr(target_os = "none", no_std, no_main)]
mod inventory;
#[cfg_attr(not(target_os = "none"), allow(dead_code))]
mod model;
#[cfg(any(target_os = "none", test))]
mod ui;
#[cfg(test)]
mod ui_tests;
#[cfg(not(target_os = "none"))]
fn main() {}
#[cfg(target_os = "none")]
mod device {
    use crate::{model, ui};
    use cmf_wp2_app::{
        Api, BUTTON, PAUSE, RESUME,
        lv::*,
        stock,
        storage::{Filesystem, Partition, Storage},
    };
    cmf_wp2_app::entry!(main);
    cmf_wp2_app::app_icon!();
    #[unsafe(no_mangle)]
    static mut STORAGE_STATS: [i32; 12] = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    #[unsafe(no_mangle)]
    static mut STORAGE_UI: [i32; 3] = [0, 0, 0]; // detail, pager page, last UI error
    static mut PARTS: [Partition; 30] = [Partition {
        name: [0; 8],
        kind: 0,
        file_id: 0,
        device: 0,
        mirror: 0,
        offset: 0,
        allocated: 0,
    }; 30];
    fn stat(i: usize, v: i32) {
        unsafe { (&raw mut STORAGE_STATS[i]).write_volatile(v) }
    }
    fn state(detail: u8, at: usize, error: i32) {
        unsafe { (&raw mut STORAGE_UI).write_volatile([detail as i32, at as i32, error]) }
    }
    fn refresh(s: &Storage<'_>) -> Option<Filesystem> {
        match s.filesystem() {
            Ok(f) if model::valid_filesystem(&f) => {
                for (i, v) in [
                    f.total,
                    f.free,
                    f.apps,
                    f.app_data,
                    f.widgets,
                    f.loader,
                    f.installed_apps,
                    f.allocation_unit,
                ]
                .into_iter()
                .enumerate()
                {
                    stat(i + 3, v as i32)
                }
                stat(11, 0);
                Some(f)
            }
            Ok(_) => {
                stat(11, -11);
                None
            }
            Err(e) => {
                stat(11, e);
                None
            }
        }
    }

    #[inline(never)]
    fn run(api: &'static Api, b: &mut Batch, lv: &Lv) -> Result<(), LvError> {
        use crate::inventory::Inventory;
        use ui::Page;
        stock::font(b)?;
        b.font(1, StockFont::F24M)?;
        b.font(2, StockFont::F48M)?;
        b.font(3, StockFont::F36M)?;
        b.submit(lv)?;
        let storage = Storage::open(api);
        let mut n = 0;
        let mut fs = None;
        let mut error = 0;
        if let Ok(s) = &storage {
            match s.partition_count() {
                Ok(count) => {
                    for i in 0..count.min(30) {
                        match s.partition(i) {
                            Ok(p) => {
                                unsafe {
                                    PARTS[i as usize] = p;
                                }
                                n = i as usize + 1;
                            }
                            Err(e) => {
                                error = e;
                                break;
                            }
                        }
                    }
                }
                Err(e) => error = e,
            }
            fs = refresh(s);
        } else if let Err(e) = &storage {
            error = *e;
        }
        stat(1, error);
        stat(2, n as i32);
        let parts = unsafe { &(&raw const PARTS).as_ref().unwrap()[..n] };
        static mut INVENTORY: Inventory = Inventory::new();
        static mut NEXT: Inventory = Inventory::new();
        let inv = unsafe { &mut *core::ptr::addr_of_mut!(INVENTORY) };
        let next = unsafe { &mut *core::ptr::addr_of_mut!(NEXT) };
        if error < 0 {
            inv.error = error;
        }
        let mut reading = false;
        let mut scan_count = None;
        let mut read_at = 0usize;
        let mut stale = false;
        if let Ok(s) = &storage {
            reading = s.begin_scan().is_ok();
            if !reading {
                inv.error = -95;
                inv.partial = true;
            }
        }
        let mut views: [Option<ui::Screen>; 4] = [None, None, None, None];
        let mut depth = 0usize;
        views[0] = Some(ui::render(
            b,
            lv,
            Page::Home,
            parts,
            fs,
            inv,
            reading,
            stale,
        )?);
        stat(0, 0);
        api.own_back(true);
        let mut paused = false;
        loop {
            let e = api.event(if reading && !paused { 16 } else { u32::MAX });
            if let Some(e) = e {
                if e.kind == PAUSE {
                    paused = true;
                    continue;
                }
                if e.kind == RESUME {
                    paused = false;
                    continue;
                }
                if paused {
                    continue;
                }
                let action = if e.kind == BUTTON {
                    ui::BACK
                } else {
                    views[depth].as_ref().unwrap().action(&e)
                };
                if action == ui::BACK {
                    if depth == 0 {
                        break;
                    }
                    let old = views[depth].take().unwrap();
                    b.del(old.root)?;
                    depth -= 1;
                    if !views[depth].as_ref().unwrap().complete && inv.ready {
                        let previous = views[depth].take().unwrap();
                        let page = previous.page;
                        b.del(previous.root)?;
                        b.submit(lv)?;
                        views[depth] =
                            Some(ui::render(b, lv, page, parts, fs, inv, reading, stale)?);
                    } else {
                        views[depth].as_ref().unwrap().visible(b, true)?;
                        b.submit(lv)?;
                    }
                    state(depth as u8, 0, 0);
                    continue;
                }
                if action == ui::REFRESH {
                    if let Ok(s) = &storage {
                        if reading {
                            s.cancel_scan().ok();
                            reading = false;
                            if inv.ready {
                                stale = true;
                            } else {
                                inv.partial = true;
                                inv.error = -125;
                            }
                        } else {
                            let fresh = refresh(s);
                            stale = fresh.is_none();
                            if fresh.is_some() {
                                fs = fresh;
                            }
                            next.reset();
                            reading = s.begin_scan().is_ok();
                            scan_count = None;
                            read_at = 0;
                        }
                    }
                    let old = views[depth].take().unwrap();
                    b.del(old.root)?;
                    b.submit(lv)?;
                    views[depth] = Some(ui::render(
                        b,
                        lv,
                        Page::Home,
                        parts,
                        fs,
                        inv,
                        reading,
                        stale,
                    )?);
                    continue;
                }
                let mut replace = false;
                let page = match action {
                    1 => Some(Page::Apps),
                    2 => Some(Page::Faces(0)),
                    3 => Some(Page::Other(0)),
                    4 => Some(Page::System),
                    5 => Some(Page::Nand),
                    6 => Some(Page::Nor),
                    8 => Some(Page::Accounting),
                    16..=31 => Some(Page::App((action - 16) as u8)),
                    32..=127 => Some(Page::File((action - 32) as u8)),
                    128..=157 => Some(Page::Partition((action - 128) as u8)),
                    200 | 201 => {
                        replace = true;
                        let delta = if action == 200 { 1 } else { -1 };
                        match views[depth].as_ref().unwrap().page {
                            Page::Faces(i) => Some(Page::Faces((i as i16 + delta).max(0) as u8)),
                            Page::Other(i) => Some(Page::Other((i as i16 + delta).max(0) as u8)),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                if let Some(page) = page {
                    if replace {
                        let old = views[depth].take().unwrap();
                        b.del(old.root)?;
                        b.submit(lv)?;
                    } else if depth < 3 {
                        views[depth].as_ref().unwrap().visible(b, false)?;
                        b.submit(lv)?;
                        depth += 1;
                    } else {
                        continue;
                    }
                    views[depth] = Some(ui::render(b, lv, page, parts, fs, inv, reading, stale)?);
                    state(depth as u8, action as usize, 0);
                }
            }
            if reading && !paused {
                if let Ok(s) = &storage {
                    if scan_count.is_none() {
                        match s.scan_step() {
                            Ok(v) if v.complete => {
                                scan_count = Some((v.files as usize).min(next.files.len()));
                                next.partial = v.partial || (v.files as usize)>next.files.len();
                                next.error = v.error;
                            }
                            Ok(_) => {}
                            Err(e) => {
                                next.error = e;
                                next.partial = true;
                                scan_count = Some(0);
                            }
                        }
                    } else if read_at < scan_count.unwrap() {
                        if let Err(e) = next.load_file(s, read_at) {
                            next.error = e;
                            next.partial = true;
                            scan_count = Some(read_at);
                        } else {
                            read_at += 1;
                        }
                    } else {
                        next.load_apps(s);
                        next.ready = true;
                        s.cancel_scan().ok();
                        if next.error < 0 && inv.ready {
                            stale = true;
                        } else {
                            inv.copy_from(next);
                        }
                        reading = false;
                        // Update the visible screen. Hidden parent pages keep their scroll position.
                        let page = views[depth].as_ref().unwrap().page;
                        let old = views[depth].take().unwrap();
                        b.del(old.root)?;
                        b.submit(lv)?;
                        views[depth] = Some(ui::render(b, lv, page, parts, fs, inv, false, stale)?);
                        stat(0, 0);
                    }
                } else {
                    reading = false;
                }
            }
        }
        if let Ok(s) = &storage {
            s.cancel_scan().ok();
        }
        Ok(())
    }
    fn main(api: &'static Api) -> i32 {
        let Some(lv) = api.lv() else { return -95 };
        let Some(mut b) = Batch::new() else {
            return -12;
        };
        let result = run(api, &mut b, &lv);
        api.own_back(false);
        match result {
            Ok(()) => 0,
            Err(e) => {
                let code = match e {
                    LvError::Code(code) => code,
                    LvError::Full => -7,
                    LvError::Budget => -12,
                };
                state(0, 0, code);
                code
            }
        }
    }
}
