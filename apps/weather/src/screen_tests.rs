extern crate std;
use super::*;
use companion_rpc as rpc;
use std::boxed::Box;
use std::vec::Vec;
use wp2_core::lvcmd::{self, Table};

// 2026-11-20 13:20
const WALL: i32 = 20_777 * 1440 + 13 * 60 + 20;
const FONTS: u8 = 0b111;

fn batch() -> Batch {
    Batch::with_buf(Box::leak(Box::new([0; BUF])))
}

/// Runs `build` with a flush that validates each batch against the loader's table and applies it; returns the
/// result and the batches' sizes.
fn run<T>(t: &mut Table, b: &mut Batch, build: impl FnOnce(&mut Batch, &mut Flush) -> Result<T, LvError>) -> (T, Vec<usize>) {
    let sizes = std::cell::RefCell::new(Vec::new());
    let mut flush = |b: &mut Batch| -> Result<(), LvError> {
        assert_eq!(lvcmd::validate(b.bytes(), t, FONTS), Ok(()));
        for r in lvcmd::records(b.bytes()) {
            lvcmd::apply(t, &r.unwrap());
        }
        sizes.borrow_mut().push(b.bytes().len());
        b.clear();
        Ok(())
    };
    let out = build(b, &mut flush).unwrap();
    flush(b).unwrap();
    (out, sizes.into_inner())
}

fn msg(id: u8, status: u8, index: u8, total: u8, record: &[u8]) -> Vec<u8> {
    [&b"R1A"[..], &[id, status, index, total], record].concat()
}

/// A model the phone answered with `recs` at WALL, in "Санкт-Петербург".
fn answered(recs: &[Vec<u8>]) -> Model {
    let mut m = Model::new();
    m.ask_now(0);
    while let Some(q) = m.poll(0) {
        if q[4] == 0x20 {
            m.reply(&msg(q[3], 0, 0, 1, &[&[0u8; 6][..], "Санкт-Петербург".as_bytes()].concat()), 10, WALL);
        } else {
            for (i, r) in recs.iter().enumerate() {
                m.reply(&msg(q[3], 0, i as u8, recs.len() as u8, r), 10, WALL);
            }
        }
    }
    m
}

/// Every hour and day with `code`, the hours at night with rain likely: the most objects a code can cost.
fn full(code: u8, hours: usize, days: i8) -> Model {
    let mut v = Vec::from([[0, 0x38, 0xFF, code, 12, 100, 200].to_vec()]); // -20.0°
    if hours > 0 {
        let mut h = Vec::from([2, 13]);
        for _ in 0..hours {
            h.extend([(-128i8) as u8, code, 0x80 | 100]);
        }
        v.push(h);
    }
    for d in 0..days {
        v.push([1, d as u8, (-128i8) as u8, 127, code, 100].to_vec());
    }
    answered(&v)
}

fn live(t: &Table) -> usize {
    (0..lvcmd::MAX_OBJS as u16).filter(|&h| t.class(h).is_some()).count()
}

#[test]
fn fonts_validate_on_their_own() {
    let mut b = batch();
    fonts(&mut b).unwrap();
    assert_eq!(lvcmd::validate(b.bytes(), &Table::new(), 0), Ok(()));
}

#[test]
fn every_icon_builds_within_four_objects() {
    for kind in [Icon::Clear, Icon::Partly, Icon::Cloud, Icon::Fog, Icon::Drizzle, Icon::Rain, Icon::Snow, Icon::Storm] {
        for night in [false, true] {
            let mut t = Table::new();
            let mut b = batch();
            run(&mut t, &mut b, |b, _| {
                theme(b)?;
                icon(b, ROOT, kind, night, 0, 0, 40)
            });
            let n = live(&t) - 1; // the root
            assert!((1..=4).contains(&n), "{kind:?} night {night}: {n}");
        }
    }
}

#[test]
fn the_pages_validate_within_the_budgets_and_free_everything() {
    // every WMO group, with all 12 hours and 7 days, then the short and empty shapes
    let mut models = Vec::new();
    for code in [0, 2, 3, 45, 55, 63, 73, 95] {
        models.push(full(code, 24, 7));
    }
    models.extend([full(63, 0, 7), full(63, 5, 0), full(63, 0, 0), full(63, 1, 1)]);
    for m in &models {
        let mut t = Table::new();
        let mut b = batch();
        let before = {
            run(&mut t, &mut b, |b, _| theme(b));
            live(&t)
        };
        for page in 0..3 {
            let (v, sizes) = run(&mut t, &mut b, |b, f| pages_view(b, f, m, WALL, page));
            assert!(sizes.iter().all(|&s| s <= BUF));
            let p = v.pager.expect("pages");
            assert_eq!((p.at, p.n), (page, 3));
            assert_eq!(v.bars[0], ROOT, "now has no scroll bar");
            for bar in &v.bars[1..] {
                assert_eq!(t.class(bar.0), Some(lvcmd::Class::ScrollBar));
            }
            assert!(live(&t) <= lvcmd::MAX_OBJS, "{} objects", live(&t));
            std::println!("page {page}: {} objects, batches {sizes:?}", live(&t));
            // leaving frees every handle
            run(&mut t, &mut b, |b, _| leave(b, v.root));
            assert_eq!(live(&t), before);
        }
        let (v, _) = run(&mut t, &mut b, |b, f| face_view(b, f, m, WALL));
        assert!(v.pager.is_none());
        run(&mut t, &mut b, |b, _| leave(b, v.root));
        assert_eq!(live(&t), before);
    }
}

#[test]
fn a_stale_cache_still_builds() {
    // three days later: no hours left, four days left, "Updated 3 d ago"
    let m = full(63, 12, 7);
    let later = WALL + 3 * 1440;
    let mut t = Table::new();
    let mut b = batch();
    run(&mut t, &mut b, |b, f| {
        theme(b)?;
        pages_view(b, f, &m, later, 1)
    });
    assert_eq!(note(&m, later).unwrap().as_str(), "Updated 3 d ago");
    assert_eq!((m.hours_len(later), m.days_len(later)), (0, 4));
}

#[test]
fn without_weather_it_says_why() {
    let mut m = Model::new();
    assert_eq!(message(&m), "Asking your phone...");
    for (s, want) in [
        (rpc::ASK, "Allow Weather in FMC Companion on your phone"),
        (rpc::DENIED, "Turn Weather on in FMC Companion"),
        (rpc::UNAVAILABLE, "No weather right now"),
    ] {
        m.ask_now(0);
        while let Some(q) = m.poll(0) {
            if q[4] == 0x30 {
                m.reply(&msg(q[3], s, 0, 0, &[]), 10, WALL);
            }
        }
        assert_eq!(message(&m), want);
    }
    // the phone never answers: after the first retry it is told what to open
    let mut m = Model::new();
    m.ask_now(0);
    while m.poll(0).is_some() {}
    while m.poll(10_000).is_some() {}
    assert_eq!(message(&m), "Open FMC Companion on your phone");
    let mut t = Table::new();
    let mut b = batch();
    for build in [pages_view as fn(&mut Batch, &mut Flush, &Model, i32, usize) -> Result<View, LvError>] {
        let (v, _) = run(&mut t, &mut b, |b, f| {
            theme(b)?;
            build(b, f, &m, WALL, 0)
        });
        assert!(v.pager.is_none());
    }
    let (v, _) = run(&mut t, &mut b, |b, f| face_view(b, f, &m, WALL));
    assert!(v.pager.is_none());
}

#[test]
fn notes_and_empty_pages() {
    let m = full(63, 0, 7);
    assert!(m.no_hours);
    assert_eq!(no_hours_text(&m), "Update FMC Companion for the hourly forecast");
    assert_eq!(note(&m, WALL).unwrap().as_str(), "Updated 200 min ago".replace("200 min", "3 h"));
    let m = full(63, 12, 7);
    assert_eq!(no_hours_text(&m), "No hours to show yet");
}

#[test]
fn bar_colours_run_cold_to_hot() {
    let c = [(-20, -10), (-2, 6), (8, 14), (15, 25), (22, 30), (30, 40)].map(|(a, b)| bar_color(a, b));
    for i in 0..c.len() {
        for j in 0..i {
            assert_ne!(c[i], c[j]);
        }
    }
    assert_eq!(bar_color(-128, -128), c[0]);
    assert_eq!(bar_color(127, 127), c[5]);
}

/// What the emulator's fake phone answers: mixed hours and days, in "Berlin" (the pager came out black there).
fn mixed(hour0: u8) -> Model {
    let hours = [(-3, 2, 10, 0), (-3, 2, 10, 0), (-4, 3, 20, 0), (-4, 3, 30, 0), (-5, 71, 60, 1), (-5, 71, 70, 1),
        (-6, 73, 80, 1), (-6, 3, 20, 1), (-6, 0, 0, 1), (-5, 0, 0, 1), (-4, 1, 10, 1), (-3, 61, 40, 1)];
    let days = [(-6, -2, 2, 20), (-8, -3, 71, 70), (-4, 1, 3, 30), (0, 5, 61, 80), (2, 8, 1, 10), (4, 11, 0, 0), (-1, 6, 95, 60)];
    let mut v = Vec::from([[0, 0xDE, 0xFF, 2, 5, 78, 3].to_vec()]);
    let mut h = Vec::from([2, hour0]);
    for (t, code, p, night) in hours {
        h.extend([t as i8 as u8, code, p | night << 7]);
    }
    v.push(h);
    for (d, (lo, hi, code, p)) in days.iter().enumerate() {
        v.push([1, d as u8, *lo as i8 as u8, *hi as i8 as u8, *code, *p].to_vec());
    }
    answered(&v)
}

/// A view is built into whatever the batch already holds, so no group of records may be longer than the room
/// asked for before it: every fill level the batch can start at, every icon.
#[test]
fn the_views_build_from_any_fill_level() {
    let mut models = Vec::from([mixed(22), mixed(9)]);
    for code in [0, 2, 3, 45, 55, 63, 73, 95] {
        models.push(full(code, 12, 7));
    }
    for (i, m) in models.iter().enumerate() {
        for fill in 0..200 {
            let mut t = Table::new();
            let mut b = batch();
            let mut flush = |b: &mut Batch| -> Result<(), LvError> {
                assert_eq!(lvcmd::validate(b.bytes(), &t, FONTS), Ok(()));
                for r in lvcmd::records(b.bytes()) {
                    lvcmd::apply(&mut t, &r.unwrap());
                }
                b.clear();
                Ok(())
            };
            for _ in 0..fill {
                if b.style(ROOT, Prop::BgColor, 0, BLACK).is_err() {
                    break; // full to the brim is a fill level too
                }
            }
            let at = b.bytes().len();
            let r = pages_view(&mut b, &mut flush, m, WALL, 0).map(|_| ());
            assert_eq!(r, Ok(()), "model {i}, {at} bytes in the batch");
            let r = face_view(&mut b, &mut flush, m, WALL).map(|_| ());
            assert_eq!(r, Ok(()), "model {i}, the face page");
        }
    }
}
