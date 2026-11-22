# AGENTS.md

Community apps for the CMF Watch Pro 2: one Rust crate per app in `apps/<name>/`, built with the
[cmf-watch-sdk](https://github.com/cmf-tech-community/cmf-watch-sdk) at the commit the `Makefile` pins
(`WP2_APPS_REV`). CI builds merged apps and publishes them to the FMC store. README.md is the human guide; this file
is what an agent needs on top.

## Read first

`make sdk` checks the pinned SDK out to `.sdk/wp2-apps`. Its docs match the SDK you build with:

- `.sdk/wp2-apps/docs/app-guide.md`: the manual (app model, `Api`, widget and page, stock look, storage, phone
  channel, budgets, the shipping checklist in section 7).
- `.sdk/wp2-apps/docs/abi-1.4.md`: the channel key K, heart rate, pager, cards, stub version; horizontal pager,
  phone music (`media_key`, `media_bind`) and the 5 s escape for `own_back` apps need the loader of 2026-11-16.
- `.sdk/wp2-apps/docs/storage-inspection.md`: read-only device storage metadata and bounded scans; requires a loader with storage service v1. Probe with `Storage::open`, not the ABI minor. Installed icons use `Batch::installed_app_icon` with `IconSize`. The desktop simulator has no LVGL; verify icon rendering in QEMU.
- `apps/vibration-lab` (template, canvas), `apps/ui-gallery` (stock LVGL widgets), `apps/sky-hop` (a game): worked
  examples. More in `.sdk/wp2-apps/sdk/examples` (`hello` and `herdr-agents` have a widget and a page).

**The SDK moves faster than the pin.** The owner publishes new loaders and stubs often, and the pinned docs only know
what existed at `WP2_APPS_REV`. Before telling anyone the watch can't do something, look at what is newer:

```bash
git -C .sdk/wp2-apps fetch --quiet origin && git -C .sdk/wp2-apps log --oneline HEAD..origin/main
```

If a newer commit has it, say so and move the pin first (skill `sdk-update`).

LVGL apps use the system font and the stock look (`cmf_wp2_app::stock`), never LVGL's defaults or an own palette.
`app-guide.md`, "Stock components: SDK and ui-gallery", maps each stock component to its SDK call. Rollers: short and
not infinite (infinite ones draw at 4-6 fps; same section).

## What an app can be

One package can offer several of these; ask which the owner wants before building.

| Surface | How | Loader |
|---|---|---|
| Menu app | the default: a row in Apps, `main` runs until `BUTTON` | any |
| Widget card | `app_features!(WIDGET)`; `api.widget(title, value)` stores text (23 B / 39 B, `\n` = 2nd line) the stub draws on a 344×166 card left of the face. A NAND write: on change, never per frame. A tap opens the app | 2026-11-15 |
| Page | `app_features!(PAGE)`: the app is a whole screen next to the face. `api.on_page()` is `true` there; no `BUTTON` (the app never exits itself), swipe away = `PAUSE`, back = `RESUME` | 2026-11-15 |
| Phone or Mac link | `send`/`recv`, `MESSAGE`: section "Apps that talk to a phone" below | 2026-11-13 for K |
| Phone services | calendar, reminders, location, weather, HTTP, notifications, media, a notification on the phone, storage, background jobs, from FMC Companion over `R1` messages (`crates/companion-rpc`): "FMC Companion" below | 2026-11-13 for K |

The user adds a widget or a page on the watch: long press on the app's row in Apps → "Add widget" / "Add page".
Details: `app-guide.md`, "Widget" and "Page". A "full-screen widget" means a page.

## Loop

```bash
make new NAME=my-app     # copy of vibration-lab
make test APP=my-app     # host unit tests
make sim APP=my-app      # simulator window: canvas apps only
make build APP=my-app    # target/my-app.wp2app
make check               # store.toml / store.md
```

- **The simulator has no LVGL** (`lv_submit` returns `-95`), no widget card and no page (`on_page()` is `false`). An
  LVGL app is tested by host tests of its batch builder (each batch validates against the loader's checker) and in
  the emulator. Pick canvas (`cmf-wp2-ui`) when the simulator matters more than stock widgets: `app-guide.md`,
  "LVGL or `cmf-wp2-ui`?".
- `make build` needs `resvg`, `llvm-objcopy` (rustup `llvm-tools`) and a C linker; `make doctor` lists what is missing.
- Emulator and watch targets (`emu`, `run`, `list`) need `CMF_WATCH_UTILS`, `STOCK_NOR` and `WP2_NETSIMD`: README.md.

## Skills

Step-by-step recipes in `.claude/skills/` (plain Markdown, any agent can follow them):

- `watch-app`: a new app or a change to one, from choosing its surfaces to the PR.
- `sdk-update`: move `WP2_APPS_REV` to a newer cmf-watch-sdk and teach this file what it brought.

## Hard rules

1. No writes to a physical watch (`make run ADDRESS=…`, `remove`, firmware) without the owner's explicit yes.
   The simulator and the emulator are fine.
2. No prebuilt `.wp2app`, `.bin`, ELF or firmware in commits; no personal data (MAC, serial, pairing secret, keys,
   GPS tracks, notification text). Placeholder MAC: `AA:BB:CC:DD:EE:FF`.
3. Nothing from cmf-watch-utils (AGPL-3.0) is copied here; this repository is MIT.
4. One app per PR. Every change to an app raises `version` in its `store.toml`, or CI fails. An SDK bump is a PR
   of its own.
5. The app stays inside the `Api` table: nothing keeps the motor, the screen or a sensor on after it exits.

## Apps that talk to a phone

Watch side: `send`/`recv` and the `MESSAGE` event (`app-guide.md`, section 4, with its protocol design rules).
Treat every message as untrusted: anyone in BLE range can read the pairing secret and send messages. For the same
reason nothing personal goes over the channel: no names, event or message titles, locations. Send counts, flags,
times of day.

Phone side, next to the Nothing X app on the same phone: the channel key K (`abi-1.4.md`, "Writing a host"). No
bind, no "Paired" popup, Nothing X keeps working. What exists today:

- The watch needs the stub with K (`fw-wp2.bin` from 2026-11-13 or later, by OTA) and a loader from 2026-11-13 or
  later. Stock firmware has no channel at all.
- Host implementations: Python (`wp2apps.channel.Channel(channel_key=True)`, Mac) and FMC Companion (below), iOS
  and Android. The Android one (a second GATT client next to Nothing X) is not yet tried on a real watch.
- A Mac and a phone can't both be connected: the watch takes one central.

An app that needs a companion says so under `## Requirements` in `store.md`, naming the companion and where to get
it.

### FMC Companion

The phone app for the watch and fmc.tools, MIT, iOS (SwiftUI) and Android (Compose):
[fmctools/companion](https://github.com/fmctools/companion). It reads the App Loader state over K next to Nothing X,
browses the FMC catalog, installs watch faces (Nothing X closed first), updates the App Loader, and answers watch
apps' requests for phone services (below). Its Apps tab lists the apps on the watch with a switch per service each
asked for.

What a watch app can count on from the companion: [docs/companion.md](docs/companion.md). That covers the Apps tab,
one BLE link with several sessions, iOS background limits (about 10 s per wake), why there are no web modules, and
building your own companion.

**Phone services.** The spec, and the only source of truth for byte layouts, is
[`docs/miniapp-rpc.md`](https://github.com/fmctools/companion/blob/main/docs/miniapp-rpc.md) in the companion repo
(`gh api repos/fmctools/companion/contents/docs/miniapp-rpc.md -H "Accept: application/vnd.github.raw"`); read it
before writing an app that uses one. Its encodings are pinned by `ios/WireKit/Tests/WireKitTests/rpc-vectors.json`
there.

Don't hand-roll `R1Q`: use **`crates/companion-rpc`** (`companion-rpc.workspace = true` in the app's `Cargo.toml`). It
builds every request below (multi-part ones too), reads replies, collects a result's records with retry and back-off
(`Call`) and decodes each method's records; `tests/vectors.rs` checks it against a copy of `rpc-vectors.json` (copy it
again when the spec changes). `make test` runs it. Worked examples: `apps/calendar` (`src/model.rs`), `apps/reminders`
(`src/model.rs`: a list call and a write call side by side, cache, pending writes).

| Method | Id | Cap | Platform | Returns / does |
|---|---|---|---|---|
| `caps` | `0x01` | none | both | the method ids this app may call now: tells "allow on your phone" from an empty result |
| `calendar.events` | `0x10` | `calendar` | both | events for 1-7 days from today: day, start, duration, flags, RGB565 colour, title |
| `reminders` | `0x11` | `reminders` | iOS | open reminders (≤ 20): id, due day and minute, flags, list colour, title |
| `reminders.complete` | `0x12` | `reminders` | iOS | marks one done |
| `location` | `0x20` | `location` | both | lat/lon in 0.1°, fix age, locality name |
| `weather` | `0x30` | `weather` | both | now, the next hours (up to 24: temperature, code, precipitation chance, night) and up to 7 days. Hours need a companion of 2026-11-20 or later: an older one answers `BAD_REQUEST` (Android) or without the hourly record (iOS), so ask again with `hours` 0 and show no hours |
| `http.get` | `0x40` | `http:<host>` | both | status, length, body (≤ 2048 B) of an `https://` GET |
| `http.request` | `0x41` | `http:<host>` | both | any method with a body (multi-part), the same reply |
| `notifications` | `0x50` | `notifications` | Android | the notifications shown now (≤ 10) |
| `notify` | `0x51` | `notify` | both | posts a notification on the phone (1 per 10 s), optionally "find my phone" |
| `storage.get/put/delete` | `0x60`-`0x62` | none | both | the app's own key/value store on the phone, 64 KiB |
| `phone.battery` | `0x70` | none | both | level and plugged |
| `media.now_playing`, `media.control` | `0x80`, `0x81` | `media` | Android | the playing track; play/pause/next/previous |
| `job.set`, `job.delete` | `0x90`, `0x91` | the job's request's | both | the phone runs a request every N min while the app is closed and `PUSH`es the result into its widget or start blob (stub 8, below) |

A method the phone's OS can't do answers `UNAVAILABLE` and `caps` never lists it: reminders on Android, notifications
and media on iOS. Both companions implement every method. Only `calendar.events` is tried on a real watch; `reminders` and
`reminders.complete` ran in the emulator against a fake phone (`apps/reminders`), the rest have no watch app, so expect
to find the first bugs.

How it works and what it can't do:

- RPC rides on app messages: every message starts with `R1`, then `Q` (request, watch → phone:
  `R1Q id method args`) or `A` (reply: `R1A id status index total record`). Other prefixes stay the app's own. A
  result of N records comes as N messages; empty or an error is one message with `total` 0. A request too big for one
  message (`http.request`, `storage.put`, `job.set`) goes as up to 9 parts; the phone answers after the last.
- The watch pulls: the phone answers the running app only, and nothing is pushed to a closed app (except a job's
  result, stub 8). Replies carry no link-level request id: ignore replies whose `id` is not your latest.
- Statuses: 0 `OK`, 1 `BAD_REQUEST`, 2 `DENIED` (the user said no, or the OS permission is off), 3 `UNAVAILABLE`
  (no fix, no network, timeout, not on this OS), 4 `ASK`, 5 `TOO_LARGE`. No answer at all (phone asleep, out of range,
  companion not running, often on iOS): show the cached copy from `store` and ask again after 3 s, then less often.
- **Allow on your phone.** Grants are per app package. The first call of a method returns `ASK` and the phone posts a
  notification (once a day per app and cap); show "Allow … in FMC Companion on your phone" and ask again every ~10 s.
  The user allows or denies on the app's page in the companion's Apps tab; after that `DENIED` means "turn it on
  there" (say where), and no new notification comes.
- Limits: a message is 1-128 bytes, a record ≤ 121; 4 messages in the inbox and the outbox, so drain `recv` every
  loop and pace multi-part sends. The phone answers within 8 s (iOS gives it about 10 s per wake). Text fields are
  UTF-8, ≤ 64 bytes; the watch font has ASCII and Cyrillic only, anything else draws as `?`. Times are the phone's
  local day + minute, no zones.
- Personal data: K is readable by anyone in BLE range. Calendar and reminder titles and notification text are off by
  default (records come with flag bit 2 and an empty title; show "busy" / "Reminder"); a per-app "Show text on the
  watch" switch turns them on. Location is 0.1° only. Event locations, attendees and notes are never sent. A watch
  app sends nothing personal either: ids, counts, flags.

**What needs a newer watch.** Patched firmware **stub 8** (published 2026-11-19 as `1.0.0.73-wp2.11`, by OTA) with the
loader of 2026-11-19 brings 16 app slots instead of 8 and `PUSH`, which `job.set` needs to refresh a widget or start
blob in the background; on an older stub the phone runs the job and drops the result. The SDK pin here predates
both (`git -C .sdk/wp2-apps log HEAD..origin/main`). The order of apps in the Apps menu is the user's, set in FMC
Companion; an app can't change it.
