use crate::{
    inventory::{App, Category, File, Inventory},
    ui::{self, Page},
};
use cmf_wp2_app::{Api, Event, Time, lv::*, stock};
use std::cell::RefCell;
use wp2_core::lvcmd::{self, Table};
thread_local! {static STATE:RefCell<(Table,u8,usize)>=RefCell::new((Table::new(),0,0));}
extern "C" fn submit(p: *const u8, n: usize) -> i32 {
    let bytes = unsafe { std::slice::from_raw_parts(p, n) };
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if let Err(e) = lvcmd::validate(bytes, &s.0, s.1) {
            return e.code();
        }
        for rec in lvcmd::records(bytes) {
            let r = rec.unwrap();
            if let lvcmd::Rec::Font { slot, .. } = r {
                s.1 |= 1 << slot;
            }
            lvcmd::apply(&mut s.0, &r);
            let live = (1..=192).filter(|&i| s.0.class(i).is_some()).count();
            s.2 = s.2.max(live);
        }
        0
    })
}
extern "C" fn rect(_: i16, _: i16, _: u16, _: u16, _: u16) {}
extern "C" fn text(_: i16, _: i16, _: *const u8, _: usize, _: u16) {}
extern "C" fn noop() {}
extern "C" fn event(_: *mut Event, _: u32) -> bool {
    false
}
extern "C" fn motor(_: u16, _: u16) -> i32 {
    0
}
extern "C" fn millis() -> u32 {
    0
}
extern "C" fn exit(_: i32) -> ! {
    std::process::abort()
}
extern "C" fn bytes(_: *const u8, _: usize) -> i32 {
    0
}
extern "C" fn bytes_mut(_: *mut u8, _: usize) -> i32 {
    0
}
extern "C" fn time(_: *mut Time) -> i32 {
    0
}
extern "C" fn battery() -> i32 {
    0
}
extern "C" fn screen(_: bool) -> i32 {
    0
}
extern "C" fn flag(_: bool) {}
fn api() -> Api {
    Api {
        abi: cmf_wp2_app::ABI,
        abi_minor: cmf_wp2_app::ABI_MINOR,
        width: 466,
        height: 466,
        fill_rect: rect,
        draw_text: text,
        present: noop,
        next_event: event,
        motor,
        millis,
        exit,
        send_msg: bytes,
        recv_msg: bytes_mut,
        load_data: bytes_mut,
        save_data: bytes,
        get_time: time,
        get_battery: battery,
        screen_on: screen,
        lv_submit: submit,
        own_back: flag,
    }
}
#[test]
fn all_screens_and_nested_full_catalog_fit_loader_budgets() {
    STATE.with(|s| *s.borrow_mut() = (Table::new(), 0, 0));
    let api = api();
    let lv = api.lv().unwrap();
    let mut b = Batch::new().unwrap();
    stock::font(&mut b).unwrap();
    b.font(1, StockFont::F24M).unwrap();
    b.font(2, StockFont::F48M).unwrap();
    b.font(3, StockFont::F36M).unwrap();
    b.submit(&lv).unwrap();
    let mut inv = Inventory::new();
    inv.ready = true;
    inv.partial = true;
    for slot in 0..16 {
        let mut name = [0; 24];
        name[..3].copy_from_slice(b"App");
        inv.apps[slot] = App {
            name,
            version: 1,
            valid: true,
            icon: true,
        };
    }
    for i in 0..96 {
        let mut path = [0; 96];
        let s = format!("/NAND:/test/{i}.wfc");
        path[..s.len()].copy_from_slice(s.as_bytes());
        inv.files[i] = File {
            path,
            size: 4096,
            category: if i < 16 {
                Category::App(i as u8)
            } else {
                Category::Face
            },
        };
    }
    inv.count = 96;
    let fs = cmf_wp2_app::storage::Filesystem {
        total: 10223616,
        free: 8000000,
        apps: 65536,
        loader: 65536,
        allocation_unit: 2048,
        ..Default::default()
    };
    let mut parts = [cmf_wp2_app::storage::Partition {
        allocated: 65536,
        device: 2,
        ..Default::default()
    }; 30];
    for (i, p) in parts.iter_mut().enumerate() {
        p.offset = i as u32 * 65536;
        p.name[..4].copy_from_slice(b"test");
    }
    let home = ui::render(&mut b, &lv, Page::Home, &parts, Some(fs), &inv, false, true).unwrap();
    home.visible(&mut b, false).unwrap();
    b.submit(&lv).unwrap();
    for missing_icons in [false, true] {
        for a in &mut inv.apps {
            a.icon = !missing_icons;
        }
        for page in [
            Page::Apps,
            Page::Faces(0),
            Page::Faces(8),
            Page::Other(0),
            Page::System,
            Page::Nand,
            Page::Nor,
            Page::Accounting,
        ] {
            let view = ui::render(&mut b, &lv, page, &parts, Some(fs), &inv, false, false).unwrap();
            view.visible(&mut b, false).unwrap();
            b.submit(&lv).unwrap();
            for child in [Page::App(15), Page::File(2), Page::Partition(0)] {
                let detail =
                    ui::render(&mut b, &lv, child, &parts, Some(fs), &inv, false, false).unwrap();
                b.del(detail.root).unwrap();
                b.submit(&lv).unwrap();
            }
            b.del(view.root).unwrap();
            b.submit(&lv).unwrap();
        }
    }
    b.del(home.root).unwrap();
    b.submit(&lv).unwrap();
    for error in [-95, -5] {
        let mut inv = Inventory::new();
        inv.error = error;
        let v = ui::render(&mut b, &lv, Page::Home, &[], None, &inv, false, false).unwrap();
        b.del(v.root).unwrap();
        b.submit(&lv).unwrap();
    }
    STATE.with(|s| {
        assert!(s.borrow().2 <= 192);
        println!("peak live objects: {}", s.borrow().2);
    });
}
