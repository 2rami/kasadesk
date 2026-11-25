//! 창들 — 창 하나에 기기 하나를 본다. 기기를 아직 안 골랐으면 고르기 화면이다. 보기·조작·HUD 는 본 앱 화면 탭과 같은
//! `kasa_castview` 다. 창마다 렌더러 하나, 연결은 그 창이 열려 있는 동안만 쥔다.

mod bottom_edge;
use bottom_edge::BottomEdge;

use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

use kasa_castview::{key_mods, Click, ScreenView, Style, Surface};
use kasa_gridview::{GridFonts, GridRenderer};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{DeviceId, ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::{CursorIcon, Fullscreen, Window, WindowAttributes, WindowId};

use crate::account::{self, Credential, Session};
use crate::login::{Action, Login};
use crate::net::Net;
use crate::rig::PickerAction;
use crate::roster::Machine;
use crate::scene::Room;
use crate::text::{Pen, Ui};

/// 보는 창의 연결이 닫히면 이만큼 뒤 다시 붙는다 — 그 기기 재시작·직통이 늦게 서는 경우. 본 앱 화면 탭과 같은 값.
const RETRY_AFTER: Duration = Duration::from_secs(3);
const FONT_SIZE: f32 = 13.0;
const PICKER_MIN: LogicalSize<f64> = LogicalSize::new(360.0, 300.0);
const BG: [u8; 4] = [24, 24, 27, 255];
const MUTE: [u8; 4] = [150, 150, 160, 255];
const ACCENT: [u8; 4] = [86, 132, 245, 255];
const EDGE: [u8; 4] = [255, 255, 255, 40];
const TEXTURE: &str = "kasadesk:screen";

pub enum Ev {
    /// 새 장·상태 — 다시 그린다.
    Wake,
    PreviewReady,
    Weather(crate::weather::Weather),
    Rgb(WindowId, u64, crate::rgb::Update),
    /// 그 기기를 연다(None 이면 고르기 창).
    Open(Option<String>),
    Roster { machines: Vec<Machine>, app_alive: bool },
    /// 로그인이 끝났다(성공이면 자격).
    Login(Result<Credential, String>),
    /// 처음 보는 Google·GitHub 신원 — 새 계정이냐 기존 계정에 잇기냐를 물어야 한다.
    Choose(account::Ticket),
    /// 관문이 켠 로그인 방법.
    Providers(Vec<String>),
    Profile(Credential, account::Profile),
    /// 관문이 자격을 거절했다(폐기·다른 곳에서 로그아웃) — 명부 일꾼이 이미 지웠다.
    Revoked,
    /// 맥 Caps Lock — winit 이 키로 안 넘겨 AppKit 감시가 보낸다.
    #[cfg(target_os = "macos")]
    CapsLock,
    /// 새 판을 받아 서명까지 확인했다(Windows — 맥은 Sparkle 이 스스로 알린다).
    #[cfg(windows)]
    UpdateReady(String),
}

/// `gpu` 가 `window` 앞에 선언된다 — surface 는 창이 살아 있을 때 먼저 놓아야 한다.
struct Win {
    gpu: GridRenderer,
    window: Arc<Window>,
    /// 보는 기기. None 이면 고르기 화면.
    label: Option<String>,
    view: Option<ScreenView>,
    /// 「화면 2/3」으로 고른 그 기기 화면 번호.
    display: Option<u32>,
    closed_since: Option<Instant>,
    cursor: (f32, f32),
    bottom_edge: BottomEdge,
    occluded: bool,
    room: Room,
    rgb: crate::rgb::Panel,
    rgb_machine: Option<String>,
    preview: crate::preview::Previews,
    preview_pending: Arc<AtomicBool>,
    ui: Ui,
}

impl Win {
    fn logical(&self) -> (f32, f32) {
        let s = self.window.inner_size();
        let scale = self.gpu.scale.max(0.5);
        (s.width as f32 / scale, s.height as f32 / scale)
    }

    fn room_cursor(&self, count: usize) {
        self.window.set_cursor(if self.room.on_help(self.cursor) || self.room.on_weather_source(self.cursor) || self.room.pointer_on_device() || self.room.pointer_on_page(count) {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        });
    }

    fn set_title(&self) {
        let title = self.label.as_deref().map_or_else(|| "KasaDesk".to_string(), |l| format!("{l} — KasaDesk"));
        self.window.set_title(&title);
    }

    fn fullscreen(&self) -> bool {
        self.window.fullscreen().is_some()
    }

    fn toggle_fullscreen(&self) {
        self.window.set_fullscreen(if self.fullscreen() { None } else { Some(Fullscreen::Borderless(None)) });
    }
}

pub struct Desk {
    // On macOS cloning a native proxy registers a run-loop source and wakes the loop.
    proxy: Arc<EventLoopProxy<Ev>>,
    net: Arc<Net>,
    windows: HashMap<WindowId, Win>,
    pending: Vec<Option<String>>,
    machines: Vec<Machine>,
    paths: Vec<String>,
    live: Vec<bool>,
    app_alive: bool,
    modifiers: ModifiersState,
    hangul: kasa_ime::Composer,
    focused: Option<WindowId>,
    test: crate::rig::Rig,
    weather: crate::weather::Weather,
    session: Arc<Session>,
    state: std::path::PathBuf,
    login: Login,
    rgb_config: Option<Arc<crate::rgb::Config>>,
    /// 받아 둔 새 판과 그 칩 자리(그린 뒤에 생긴다).
    #[cfg(windows)]
    update: Option<(String, Option<(f32, f32, f32, f32)>)>,
    /// 새 판 설치를 맡겼다 — 설치기가 실행 파일을 갈아 끼우게 다음 바퀴에 끝낸다.
    #[cfg(windows)]
    quit_for_update: bool,
}

impl Desk {
    pub fn new(proxy: EventLoopProxy<Ev>, net: Arc<Net>, open: Option<String>, session: Arc<Session>, state: std::path::PathBuf) -> Self {
        let test = crate::rig::Rig::from_env();
        let mut login = Login::default();
        if test.login_panel { login.open_login(); }
        crate::weather::start(&net.handle, proxy.clone());
        let desk = Self {
            proxy: Arc::new(proxy),
            net,
            windows: HashMap::new(),
            pending: vec![open],
            weather: Default::default(),
            machines: Vec::new(),
            paths: Vec::new(),
            live: Vec::new(),
            app_alive: false,
            modifiers: ModifiersState::empty(),
            hangul: kasa_ime::Composer::new(),
            focused: None,
            test,
            session,
            state,
            login,
            rgb_config: crate::rgb::Config::load(),
            #[cfg(windows)]
            update: None,
            #[cfg(windows)]
            quit_for_update: false,
        };
        if desk.login.is_open() {
            desk.fetch_providers();
        }
        desk
    }

    fn fetch_providers(&self) {
        let proxy = self.proxy.clone();
        self.net.handle.spawn(async move {
            let list = account::providers(&account::client(), &account::gateway()).await;
            let _ = proxy.send_event(Ev::Providers(list));
        });
    }

    fn redraw_pickers(&self) {
        for w in self.windows.values().filter(|w| w.label.is_none()) {
            w.window.request_redraw();
        }
    }

    /// 로그인 판이 부른 일 — 관문 일은 카사넷 런타임에서 돌리고 결과를 창 사건으로 받는다.
    fn login_action(&mut self, action: Action) {
        match action {
            Action::None => return,
            Action::Redraw => {}
            Action::Opened => self.fetch_providers(),
            Action::ProfileOpened => {
                if let Some(cred) = self.session.get() {
                    let proxy = self.proxy.clone();
                    self.net.handle.spawn(async move {
                        if let Some(info) = account::profile(&cred).await {
                            let _ = proxy.send_event(Ev::Profile(cred, info));
                        }
                    });
                }
            }
            Action::Logout => {
                if let Some(cred) = self.session.get() {
                    self.net.handle.spawn(async move { account::logout(&account::client(), &cred).await });
                }
                account::clear(&self.state);
                self.session.set(None);
            }
            Action::Provider(provider) => {
                let (proxy, machine) = (self.proxy.clone(), account::install_id(&self.state));
                self.net.handle.spawn(async move {
                    let r = account::oauth_login(&account::client(), &account::gateway(), &provider, &machine, account::open_browser).await;
                    let _ = proxy.send_event(match r {
                        Ok(account::Outcome::Done(c)) => Ev::Login(Ok(c)),
                        Ok(account::Outcome::Choose(t)) => Ev::Choose(t),
                        Err(e) => Ev::Login(Err(e)),
                    });
                });
            }
            Action::Signup | Action::Claim(..) => {
                let Some(ticket) = self.login.choose.as_ref().map(|t| t.ticket.clone()) else { return };
                let proxy = self.proxy.clone();
                self.net.handle.spawn(async move {
                    let (client, origin) = (account::client(), account::gateway());
                    let r = match action {
                        Action::Claim(id, pw) => account::oauth_claim(&client, &origin, &ticket, &id, &pw).await,
                        _ => account::oauth_signup(&client, &origin, &ticket).await,
                    };
                    let _ = proxy.send_event(Ev::Login(r));
                });
            }
            Action::Password(id, pw) => {
                let (proxy, machine) = (self.proxy.clone(), account::install_id(&self.state));
                self.net.handle.spawn(async move {
                    let r = account::password_login(&account::client(), &account::gateway(), &id, &pw, &machine).await;
                    let _ = proxy.send_event(Ev::Login(r));
                });
            }
        }
        self.redraw_pickers();
    }

    /// 그 기기를 연다 — 이미 보는 창이 있으면 그 창을 앞으로, 고르기 창이 있으면 거기서, 없으면 새 창.
    fn open(&mut self, event_loop: &ActiveEventLoop, label: Option<String>) {
        if let Some(w) = label.as_ref().and_then(|l| self.windows.values().find(|w| w.label.as_ref() == Some(l))) {
            w.window.focus_window();
            return;
        }
        if label.is_none() {
            if let Some(w) = self.windows.values().next() {
                w.window.focus_window();
                return;
            }
        }
        if let Some(w) = self.windows.values_mut().find(|w| w.label.is_none()) {
            w.label = label;
            w.preview = Default::default();
            w.preview_pending = Default::default();
            w.rgb = Default::default();
            w.room.suspend();
            w.window.set_min_inner_size(None::<LogicalSize<f64>>);
            w.window.set_cursor(CursorIcon::Default);
            w.set_title();
            w.window.focus_window();
            w.window.request_redraw();
            return;
        }
        let attrs = WindowAttributes::default()
            .with_title("KasaDesk")
            .with_inner_size(LogicalSize::new(1280.0, 800.0));
        let attrs = if label.is_none() { attrs.with_min_inner_size(PICKER_MIN) } else { attrs };
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("[kasadesk] 창을 못 만든다: {e}");
                return;
            }
        };
        // 본 앱과 같은 입력기 길 — macOS 는 이 앱이 한글을 조합하고, 그 밖은 OS 입력기.
        window.set_ime_allowed(cfg!(not(target_os = "macos")));
        let gpu = match GridRenderer::new(window.clone(), FONT_SIZE, GridFonts { primary: None, bundled: Vec::new() }) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("[kasadesk] 렌더러를 못 만든다: {e:#}");
                return;
            }
        };
        let mut win = Win {
            gpu, window, label, view: None, display: None, closed_since: None,
            cursor: (0.0, 0.0), bottom_edge: BottomEdge::default(), occluded: false,
            room: Room::default(), rgb:Default::default(), rgb_machine:None, preview: Default::default(), preview_pending: Default::default(), ui: Ui::new(),
        };
        if win.label.is_none() {
            win.room.reflow(win.logical(), &self.machines);
        }
        win.set_title();
        win.window.request_redraw();
        self.windows.insert(win.window.id(), win);
    }

    /// 보는 창만 연결을 쥔다. 닫힌 연결은 `RETRY_AFTER` 뒤 다시 붙고, 그동안의 까닭은 이어 보인다.
    fn sync(&mut self) {
        let now = Instant::now();
        for w in self.windows.values_mut() {
            let Some(label) = w.label.clone() else { continue };
            let previous = match &w.view {
                Some(v) if !v.closed() => {
                    w.closed_since = None;
                    continue;
                }
                Some(v) => {
                    let since = *w.closed_since.get_or_insert(now);
                    if now.duration_since(since) < RETRY_AFTER {
                        continue;
                    }
                    Some(v.status())
                }
                None => None,
            };
            w.bottom_edge.clear();
            w.view = None;
            w.gpu.drop_image(TEXTURE);
            let proxy = self.proxy.clone();
            let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || drop(proxy.send_event(Ev::Wake)));
            let fit = w.window.available_monitors().map(|m| m.size()).fold((0, 0), |a, s| if s.width * s.height > a.0 * a.1 { (s.width, s.height) } else { a });
            let start = self.net.start_view(&label);
            if let Some(view) = ScreenView::open(&label, fit, w.display, self.test.silent_audio, start, wake) {
                if let Some(previous) = previous {
                    view.carry_status(&previous);
                }
                w.view = Some(view);
                w.closed_since = None;
            }
            w.window.request_redraw();
        }
    }

    fn paint(&mut self, id: WindowId) {
        let Some(w) = self.windows.get_mut(&id) else { return };
        let (ww, wh) = w.logical();
        // 글리프 아틀라스 재배치는 이 프레임이 글자 사각형을 하나라도 쌓기 전에만 안전하다 — 쌓인 뒤면 옛 자리를 가리킨다.
        w.gpu.maintain_atlas();
        w.gpu.clear_chrome();
        self.test.capture(&mut w.gpu);
        let result = if w.label.is_none() {
            let origin = match (self.app_alive, self.session.get().is_some()) {
                (true, true) => "카사텀 · 계정 명부",
                (true, false) => "카사텀 명부",
                (false, true) => "계정 명부",
                (false, false) => "저장된 명부",
            };
            let foot = format!("기기 {}대  ·  {origin}", self.machines.len());
            let (login, cred) = (&mut self.login, self.session.get());
            #[cfg(windows)]
            let update = &mut self.update;
            w.preview_pending.store(false, Ordering::Release);
            let pictures: Vec<_> = w.room.preview_targets().iter().filter_map(|&(i, _)| {
                Some((i, w.preview.picture(self.machines.get(i)?)?))
            }).collect();
            let index = w.room.rgb_index(&self.machines, self.rgb_config.as_deref(), w.rgb_machine.as_deref());
            let route = index.and_then(|i| crate::rgb::Target::for_machine(&self.machines[i], cred.as_ref(), self.rgb_config.as_ref()));
            let hint = route.is_none().then_some(if cred.is_some() {
                "집 PC의 카사텀과 RGB Control 연결을 확인하세요"
            } else {
                "집 PC와 같은 카사계정으로 로그인하세요"
            });
            w.rgb.discard_stale(route.as_ref());
            let snap = w.rgb.data.clone();
            let account_devices = self.machines.iter().filter(|m| m.account.is_some()).count();
            let painted = w.room.paint(&pictures, index.zip(snap.as_deref()), index.zip(w.rgb.open.then_some(w.rgb.part())), index.zip(w.rgb.gpu_color()), &mut w.gpu, &mut w.ui, &self.machines, &self.paths, &self.live, &foot, self.app_alive, self.weather, |g, ui, size| {
                #[cfg(windows)]
                if let Some((version, rect)) = update.as_mut().filter(|_| !w.rgb.open && !login.is_open()) {
                    *rect = Some(paint_update(g, ui, size.0, version));
                }
                w.rgb.paint(g, ui, size, index.is_some() && !login.signing_in(), hint);
                let show_account = !w.rgb.open || size.0 >= 550.0 || login.is_open();
                login.paint_chip(g, ui, size.0, cred.as_ref(), show_account);
                login.paint_panel(g, ui, size, cred.as_ref(), account_devices);
            });
            if painted.is_ok() { self.test.presented_previews(&pictures); }
            painted
        } else {
            w.gpu.rect(0.0, 0.0, ww, wh, BG);
            let fullscreen = w.fullscreen();
            if let Some(view) = w.view.as_mut() {
                let extra = ["기기", if fullscreen { "창으로" } else { "전체 화면" }];
                let style = Style { accent: ACCENT, mute: MUTE, edge: EDGE, extra: &extra };
                let mut pen = Pen { g: &mut w.gpu, ui: &mut w.ui };
                kasa_castview::paint(&mut pen, view, TEXTURE, (0.0, 0.0, ww, wh), Surface::Window(id), w.cursor, &style);
            }
            w.gpu.render(0.0, true, |_, _, _, _, _, _| {})
        };
        if let Err(e) = result {
            eprintln!("[kasadesk] 그리기 실패: {e:#}");
        }
        // 아틀라스가 차서 빈 글자가 났으면 재배치하고 한 번 더 그린다.
        if w.gpu.atlas_needs_another_frame() {
            w.window.request_redraw();
        }
    }

    fn back_to_picker(w: &mut Win, machines: &[Machine]) {
        w.bottom_edge.clear();
        w.view = None;
        let label = w.label.take();
        w.display = None;
        w.closed_since = None;
        w.gpu.drop_image(TEXTURE);
        w.window.set_min_inner_size(Some(PICKER_MIN));
        w.room.reflow(w.logical(), machines);
        if let Some(i) = label.and_then(|l| machines.iter().position(|m| m.label == l)) {
            w.room.surface(i);
        }
        w.set_title();
        w.window.request_redraw();
    }

    fn mouse(&mut self, id: WindowId, event: &WindowEvent) {
        let mods = key_mods(self.modifiers);
        let modifiers = self.modifiers;
        let Some(w) = self.windows.get_mut(&id) else { return };
        let p = (f64::from(w.cursor.0), f64::from(w.cursor.1));
        let Some(view) = w.view.as_mut() else {
            if w.label.is_some() {
                return;
            }
            if let WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } = event {
                #[cfg(windows)]
                if !self.login.is_open() && !w.rgb.open && self.update.as_ref().and_then(|u| u.1).is_some_and(|(x, y, w, h)| {
                    p.0 >= f64::from(x) && p.0 < f64::from(x + w) && p.1 >= f64::from(y) && p.1 < f64::from(y + h)
                }) {
                    self.quit_for_update = crate::update::install_now(true);
                    return;
                }
                let logged_in = self.session.get().is_some();
                let action = self.login.click(p, logged_in);
                if !matches!(action, Action::None) || self.login.is_open() {
                    w.room.stop_navigation();
                    self.login_action(action);
                    return;
                }
            }
            if matches!(event, WindowEvent::CursorMoved { .. }) && self.login.clickable(p) {
                w.window.set_cursor(CursorIcon::Pointer);
                return;
            }
            if self.login.is_open() {
                w.room.stop_navigation();
                w.window.set_cursor(CursorIcon::Default);
                return;
            }
            if matches!(event, WindowEvent::MouseInput { button: MouseButton::Left, state: ElementState::Released, .. } | WindowEvent::CursorLeft { .. }) {
                w.rgb.end_slide();
            }
            if matches!(event, WindowEvent::CursorMoved { .. }) && w.rgb.slide(w.cursor) {
                w.window.request_redraw();
                return;
            }
            let rgb_index = w.room.rgb_index(&self.machines, self.rgb_config.as_deref(), w.rgb_machine.as_deref());
            let left = matches!(event, WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. });
            if left && !w.rgb.open && w.room.click_help(w.cursor) {
                w.window.request_redraw();
                return;
            }
            if left && !w.rgb.open && w.room.on_weather_source(w.cursor) {
                account::open_browser("https://open-meteo.com/");
                return;
            }
            if left && !w.rgb.open && !w.rgb.clickable(w.cursor) && !w.room.pointer_on_page(self.machines.len()) {
                if let Some(crate::scene::Pick::Case(index)) = w.room.pick(w.cursor) {
                    let fan = w.room.light_hit(index, w.cursor).unwrap_or(crate::rgb::LightPart::Fan(0));
                    let route = crate::rgb::Target::for_machine(&self.machines[index], self.session.get().as_ref(), self.rgb_config.as_ref());
                    w.rgb.discard_stale(route.as_ref());
                    w.rgb_machine = Some(self.machines[index].label.clone());
                    w.rgb.choose(fan);
                    w.room.rgb_view(Some(index));
                    w.window.request_redraw();
                    return;
                }
            }
            if let Some(index) = rgb_index {
                if left {
                    let was_open = w.rgb.open;
                    let touched = w.rgb.click(w.cursor);
                    if !touched {
                        if let Some(fan) = w.room.light_hit(index, w.cursor) {
                            w.rgb.choose(fan);
                        } else if was_open {
                            w.rgb.close();
                        }
                    }
                    if was_open || touched || w.rgb.open {
                        w.rgb_machine = w.rgb.open.then(|| self.machines[index].label.clone());
                        w.room.rgb_view(w.rgb.open.then_some(index));
                        w.window.request_redraw();
                        return;
                    }
                }
                if w.rgb.open || w.rgb.contains(w.cursor) {
                    let clickable = w.rgb.clickable(w.cursor) || w.room.light_hit(index, w.cursor).is_some();
                    w.window.set_cursor(if clickable { CursorIcon::Pointer } else { CursorIcon::Default });
                    return;
                }
            }
            let redraw = match event {
                WindowEvent::CursorMoved { .. } => w.room.pointer(Some(w.cursor)),
                WindowEvent::CursorLeft { .. } => w.room.pointer(None),
                WindowEvent::MouseInput { state, button: MouseButton::Right, .. } => {
                    w.room.drag((*state == ElementState::Pressed).then_some(w.cursor))
                }
                WindowEvent::MouseWheel { delta, .. } => w.room.scroll(match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(d) => d.y as f32 / (45.0 * w.gpu.scale.max(0.5)),
                }),
                WindowEvent::PinchGesture { delta, .. } => w.room.scroll(*delta as f32 * 18.0),
                WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                    if w.room.click_page(&self.machines) {
                        true
                    } else {
                        if let Some(index) = w.room.hit() {
                            if w.room.dive(index) {
                                w.window.set_cursor(CursorIcon::Default);
                                w.window.request_redraw();
                            }
                        }
                        return;
                    }
                }
                _ => return,
            };
            w.room_cursor(self.machines.len());
            if redraw {
                w.window.request_redraw();
            }
            return;
        };
        let redraw = match event {
            WindowEvent::MouseInput { state, button, .. } => {
                let bit = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 4,
                    _ => return,
                };
                let pressed = *state == ElementState::Pressed;
                if view.menu_click(p, bit, pressed) {
                    true
                } else {
                    match view.click(p, bit, pressed, mods) {
                        Click::Ignored => false,
                        Click::Handled | Click::Pressed => true,
                        Click::Display(next) => {
                            w.bottom_edge.clear();
                            w.display = Some(next);
                            w.view = None;
                            true
                        }
                        Click::Extra(0) => {
                            Self::back_to_picker(w, &self.machines);
                            true
                        }
                        Click::Extra(_) => {
                            w.toggle_fullscreen();
                            true
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { .. } => view.hover(p, mods) && view.key_menu.is_some(),
            WindowEvent::MouseWheel { delta, .. } => view.wheel(p, delta, modifiers),
            WindowEvent::PinchGesture { delta, .. } => view.pinch(p, *delta),
            _ => false,
        };
        if redraw {
            w.window.request_redraw();
        }
    }

    fn key(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: &winit::event::KeyEvent) {
        let m = self.modifiers;
        // 창 닫기·붙여넣기·전체 화면은 이 앱이 받는다 — 본 앱 꺼낸 화면 창과 같은 손가락(맥 ⌘, Windows Ctrl+Shift — 붙여넣기는 Ctrl+V 도).
        let host = if cfg!(target_os = "macos") { m.super_key() } else { m.control_key() && m.shift_key() };
        let pressed = event.state == ElementState::Pressed;
        let code = match event.physical_key {
            PhysicalKey::Code(c) => Some(c),
            PhysicalKey::Unidentified(_) => None,
        };
        if pressed && host && code == Some(KeyCode::KeyW) {
            self.windows.remove(&id);
            if self.windows.is_empty() {
                event_loop.exit();
            }
            return;
        }
        let Some(w) = self.windows.get_mut(&id) else { return };
        if pressed && cfg!(target_os = "macos") && m.super_key() && m.control_key() && code == Some(KeyCode::KeyF) {
            w.toggle_fullscreen();
            return;
        }
        let Some(view) = w.view.as_mut() else {
            if w.label.is_none() {
                if !pressed {
                    if let Some(code) = code { w.room.travel_key(code, false); }
                }
                if self.login.is_open() { w.room.stop_navigation(); }
                if let Some(action) = self.login.key(event) {
                    self.login_action(action);
                    return;
                }
            }
            if w.rgb.open && w.label.is_none() {
                if pressed {
                    if code == Some(KeyCode::KeyV) && (m.super_key() || m.control_key()) && w.rgb.editing_hex() {
                        if let Ok(value) = arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                            w.rgb.paste_hex(&value);
                        }
                    } else if let Some(code) = code {
                        w.rgb.key(code, event.text.as_deref(), m.shift_key());
                    }
                    if !w.rgb.open {
                        w.rgb_machine = None;
                        w.room.rgb_view(None);
                    }
                    w.window.request_redraw();
                }
                return;
            }
            if !pressed || w.label.is_some() || m.super_key() || m.control_key() || m.alt_key() {
                return;
            }
            if let Some(code) = code {
                if code == KeyCode::KeyL && !event.repeat {
                    if let Some(i) = w.room.rgb_index(&self.machines, self.rgb_config.as_deref(), w.rgb_machine.as_deref()) {
                        w.rgb_machine = Some(self.machines[i].label.clone());
                        w.rgb.choose(crate::rgb::LightPart::Fan(0));
                        w.room.rgb_view(Some(i));
                        w.window.request_redraw();
                        return;
                    }
                }
                if w.room.travel_key(code, pressed) { w.window.request_redraw(); return; }
                self.picker_key(id, code, event.repeat);
            }
            return;
        };
        let paste = pressed && code == Some(KeyCode::KeyV) && (host || (cfg!(windows) && m.control_key() && !m.alt_key()));
        if paste {
            view.forget_alone();
            if !event.repeat && view.control {
                if let Some(text) = arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok()) {
                    view.commit(&text);
                }
            }
            return;
        }
        if view.key_event(&mut self.hangul, event, m) {
            w.window.request_redraw();
        }
    }

    fn picker_key(&mut self, id: WindowId, code: KeyCode, repeat: bool) {
        let Some(w) = self.windows.get_mut(&id).filter(|w| w.label.is_none()) else { return };
        if (code == KeyCode::F1 || (code == KeyCode::Escape && w.room.help_open())) && !repeat {
            w.room.toggle_help();
            w.window.request_redraw();
        } else if code == KeyCode::KeyR && !repeat {
            w.room.toggle_rain();
            w.window.request_redraw();
        } else if matches!(code, KeyCode::Enter | KeyCode::NumpadEnter) {
            if !repeat {
                if let Some(index) = w.room.selected() {
                    if w.room.dive(index) {
                        w.window.request_redraw();
                    }
                }
            }
        } else if w.room.keyboard(code, &self.machines) {
            w.window.request_redraw();
        }
    }

    fn pointer_event(&mut self, id: WindowId, event: &WindowEvent) {
        let Some(w) = self.windows.get_mut(&id) else { return };
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let scale = f64::from(w.gpu.scale.max(0.5));
                let p = (position.x / scale, position.y / scale);
                w.cursor = (p.0 as f32, p.1 as f32);
                if let Some(view) = w.view.as_mut() {
                    let active = self.focused == Some(id) && !w.occluded && view.control
                        && view.key_menu.is_none() && !view.closed() && view.input_note().is_none();
                    if let Some(hit) = w.bottom_edge.motion(view.layout.filter(|_| active), p, scale) {
                        // Clicks and scrolling still go through the normal in-image hit test.
                        if !hit.crossed || w.window.set_cursor_position(PhysicalPosition::new(position.x, hit.local_row)).is_ok() {
                            w.cursor.1 = hit.image_row;
                        }
                        view.pointer((p.0, hit.remote_y), view.buttons, view.remote_mods(key_mods(self.modifiers)));
                        return;
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => w.bottom_edge.clear(),
            _ => {}
        }
        self.mouse(id, event);
    }

    fn picker_probe(&mut self, action: PickerAction) {
        let Some((&id, w)) = self.windows.iter().find(|(_, w)| w.label.is_none()) else { return };
        let (x, y, click) = match action {
            PickerAction::Snapshot(path) => {
                if let Some(w) = self.windows.get_mut(&id) {
                    w.gpu.capture_next = Some(path.to_string_lossy().into_owned());
                    w.window.request_redraw();
                }
                return;
            }
            PickerAction::HandOff => {
                self.test.release_picker();
                return;
            }
            PickerAction::Resize(width, height) => {
                let _ = w.window.request_inner_size(LogicalSize::new(f64::from(width), f64::from(height)));
                return;
            }
            PickerAction::Hover(x, y) => (x, y, false),
            PickerAction::Click(x, y) => (x, y, true),
            PickerAction::Travel(code, pressed) => {
                if let Some(w) = self.windows.get_mut(&id) { w.room.travel_key(code, pressed); w.window.request_redraw(); }
                return;
            }
            PickerAction::Scroll(steps) => {
                self.pointer_event(id, &WindowEvent::MouseWheel { device_id: DeviceId::dummy(),
                    delta: MouseScrollDelta::LineDelta(0.0, steps), phase: winit::event::TouchPhase::Moved });
                return;
            }
            PickerAction::Drag(pressed) => {
                self.pointer_event(id, &WindowEvent::MouseInput { device_id: DeviceId::dummy(), button: MouseButton::Right,
                    state: if pressed { ElementState::Pressed } else { ElementState::Released } });
                return;
            }
            PickerAction::Key(code) => {
                self.picker_key(id, code, false);
                return;
            }
        };
        let size = w.window.inner_size();
        let position = PhysicalPosition::new(x * f64::from(size.width), y * f64::from(size.height));
        let device_id = DeviceId::dummy();
        self.pointer_event(id, &WindowEvent::CursorMoved { device_id, position });
        if click {
            self.pointer_event(id, &WindowEvent::MouseInput {
                device_id, state: ElementState::Pressed, button: MouseButton::Left,
            });
        }
    }
}

impl ApplicationHandler<Ev> for Desk {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // NSApplication 이 선 뒤에 건다(한 번만 걸린다).
        #[cfg(target_os = "macos")]
        {
            static STARTED: std::sync::Once = std::sync::Once::new();
            STARTED.call_once(crate::update::start);
            let proxy = self.proxy.clone();
            kasa_castview::watch_caps_lock(move || drop(proxy.send_event(Ev::CapsLock)));
        }
        for open in std::mem::take(&mut self.pending) {
            self.open(event_loop, open);
        }
        if let Some((provider, display)) = self.test.choose.take() {
            self.login.ask(account::Ticket { ticket: String::new(), provider, display, signup: true });
        }
        if let Some((id, pw)) = self.test.login.take() {
            self.login_action(Action::Password(id, pw));
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Ev) {
        match event {
            Ev::Rgb(id, generation, data) => {
                if let Some(w) = self.windows.get_mut(&id).filter(|w| w.label.is_none()) {
                    w.rgb.receive(generation, data);
                    w.window.request_redraw();
                }
            }
            Ev::Weather(weather) => {
                self.weather = weather;
                self.redraw_pickers();
            }
            Ev::PreviewReady => {} // about_to_wait schedules one room draw for all pending monitors.
            Ev::Wake => {
                for w in self.windows.values().filter(|w| w.label.is_some()) {
                    w.window.request_redraw();
                }
            }
            Ev::Open(label) => self.open(event_loop, label),
            Ev::Login(result) => {
                let result = result.and_then(|c| {
                    account::save(&self.state, &c).map_err(|e| format!("로그인 정보를 못 남겼어요: {e}"))?;
                    eprintln!("[kasadesk] 계정 {} 로그인", c.account);
                    self.session.set(Some(c));
                    Ok(())
                });
                self.login.done(result);
                self.redraw_pickers();
            }
            Ev::Choose(ticket) => {
                self.login.ask(ticket);
                self.redraw_pickers();
            }
            Ev::Providers(list) => {
                self.login.providers = list;
                self.redraw_pickers();
            }
            Ev::Profile(owner, info) => {
                if self.session.get().as_ref() == Some(&owner) {
                    self.login.set_profile(info);
                    self.redraw_pickers();
                }
            }
            Ev::Revoked => {
                self.login.close();
                self.redraw_pickers();
            }
            #[cfg(windows)]
            Ev::UpdateReady(version) => {
                self.update = Some((version, None));
                self.redraw_pickers();
            }
            Ev::Roster { machines, app_alive } => {
                let (paths, live): (Vec<_>, Vec<_>) = machines.iter().map(|m| {
                    let (path, up) = self.net.path_status(&m.label);
                    (if m.fresh { path } else { format!("{path} · 지난 주소") }, up)
                }).unzip();
                let reflow = machines.len() != self.machines.len() || machines.iter().zip(&self.machines)
                    .any(|(m, old)| m.label != old.label || m.kind != old.kind || m.model_id != old.model_id || m.rgb_owner != old.rgb_owner || m.kasanet.get("id") != old.kasanet.get("id"));
                let redraw = reflow || paths != self.paths || live != self.live || app_alive != self.app_alive;
                self.machines = machines;
                self.paths = paths;
                self.live = live;
                self.app_alive = app_alive;
                if redraw {
                    for w in self.windows.values_mut().filter(|w| w.label.is_none()) {
                        if reflow {
                            w.room.reflow(w.logical(), &self.machines);
                            w.room_cursor(self.machines.len());
                        }
                        w.window.request_redraw();
                    }
                }
            }
            #[cfg(target_os = "macos")]
            Ev::CapsLock => {
                if let Some(view) = self.focused.and_then(|id| self.windows.get(&id)).and_then(|w| w.view.as_ref()) {
                    view.caps_lock();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match &event {
            WindowEvent::CloseRequested => {
                self.windows.remove(&id);
                if self.windows.is_empty() {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => self.paint(id),
            WindowEvent::Resized(size) => {
                if let Some(w) = self.windows.get_mut(&id) {
                    w.bottom_edge.clear();
                    w.gpu.resize(size.width, size.height);
                    if w.label.is_none() {
                        w.room.reflow(w.logical(), &self.machines);
                        w.room_cursor(self.machines.len());
                    }
                    w.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(w) = self.windows.get_mut(&id) {
                    let old_scale = w.gpu.scale.max(0.5);
                    w.gpu.set_scale(w.window.scale_factor() as f32);
                    let ratio = old_scale / w.gpu.scale.max(0.5);
                    w.cursor = (w.cursor.0 * ratio, w.cursor.1 * ratio);
                    if w.label.is_none() {
                        w.room.rescale_pointer(ratio);
                    }
                    w.gpu.set_font_size(FONT_SIZE);
                    let size = w.window.inner_size();
                    w.bottom_edge.clear();
                    w.gpu.resize(size.width, size.height);
                    if w.label.is_none() {
                        w.room.reflow(w.logical(), &self.machines);
                        w.room_cursor(self.machines.len());
                    }
                    w.window.request_redraw();
                }
            }
            WindowEvent::Occluded(hidden) => {
                if let Some(w) = self.windows.get_mut(&id) {
                    w.bottom_edge.clear();
                    w.occluded = *hidden;
                    if !hidden { w.window.request_redraw(); }
                }
            }
            WindowEvent::Focused(f) => {
                // 창을 오가는 사이 조합키(⌘Tab 의 ⌘)를 뗀 것은 이 창에 안 온다 — 들고 있던 값을 버린다. 안 버리면 돌아와 누르는
                // 클릭·글자가 그 기기에 Alt 등이 눌린 채로 가서 조작이 먹통처럼 보인다.
                self.modifiers = ModifiersState::empty();
                if *f {
                    self.focused = Some(id);
                } else if self.focused == Some(id) {
                    if let Some(w) = self.windows.get_mut(&id) {
                        w.bottom_edge.clear();
                        w.room.stop_navigation();
                        if let Some(view) = w.view.as_mut() {
                            view.focus_lost();
                        }
                    }
                    self.focused = None;
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                let event = event.clone();
                self.key(event_loop, id, &event);
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                match self.windows.get(&id) {
                    Some(w) if w.label.is_none() => {
                        if self.login.commit(text) {
                            w.window.request_redraw();
                        }
                    }
                    Some(w) => {
                        if let Some(view) = w.view.as_ref() {
                            view.commit(text);
                        }
                    }
                    None => {}
                }
            }
            WindowEvent::CursorMoved { .. } | WindowEvent::CursorLeft { .. } | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. } | WindowEvent::PinchGesture { .. } => {
                // On a rig the hardware cursor can leave the window during a scripted hover.
                if !self.test.drives_picker() || self.windows.get(&id).is_some_and(|w| w.label.is_some()) {
                    self.pointer_event(id, &event);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(windows)]
        if self.quit_for_update {
            event_loop.exit();
            return;
        }
        for action in self.test.take_picker_actions(Instant::now()) {
            self.picker_probe(action);
        }
        self.sync();
        let mut views: Vec<&mut ScreenView> = self.windows.values_mut().filter_map(|w| w.view.as_mut()).collect();
        if self.test.tick(&mut views) {
            event_loop.exit();
            return;
        }
        let now = Instant::now();
        let mut next = self.test.next_wake(now);
        if self.windows.values().any(|w| w.label.is_some()) {
            schedule(&mut next, now + Duration::from_millis(500));
        }
        if self.test.capture_due(now) {
            if let Some(w) = self.windows.values().next() {
                w.window.request_redraw();
            }
        }
        let arrived: Vec<String> = self.windows.values_mut()
            .filter(|w| w.label.is_none())
            .filter_map(|w| w.room.take_arrival())
            .filter_map(|i| self.machines.get(i).map(|m| m.label.clone()))
            .collect();
        for label in arrived {
            self.open(event_loop, Some(label));
        }
        for (&id, w) in self.windows.iter_mut().filter(|(_, w)| w.label.is_none()) {
            if !w.rgb.open { w.rgb_machine = None; }
            let index = w.room.rgb_index(&self.machines, self.rgb_config.as_deref(), w.rgb_machine.as_deref());
            let has_pc = index.is_some();
            if !has_pc || self.login.signing_in() {
                if w.rgb.open {
                    w.rgb.close();
                    w.room.rgb_view(None);
                    w.window.request_redraw();
                }
            }
            let active = has_pc && !self.login.signing_in() && (self.focused == Some(id) || self.test.drives_picker());
            let rgb_proxy = self.proxy.clone();
            let route = index.filter(|_| !self.login.signing_in()).and_then(|i| {
                crate::rgb::Target::for_machine(&self.machines[i], self.session.get().as_ref(), self.rgb_config.as_ref())
            });
            if w.rgb.sync(active, &self.net.handle, route, move |gen, msg| {
                let _ = rgb_proxy.send_event(Ev::Rgb(id, gen, msg));
            }) {
                w.window.request_redraw();
            }
            if let Some(at)=w.rgb.retry_at(){schedule(&mut next,at);}
            let hidden = (w.occluded || w.window.is_minimized() == Some(true)) && !self.test.captures_previews();
            let visible = if hidden || self.login.signing_in() { &[] } else { w.room.preview_targets() };
            let proxy = self.proxy.clone();
            let pending = w.preview_pending.clone();
            let (changed, retry_at) = w.preview.sync(visible, &self.machines, &self.live, &self.net, move || {
                if !pending.swap(true, Ordering::AcqRel) { let _ = proxy.send_event(Ev::PreviewReady); }
            });
            if changed { w.window.request_redraw(); }
            if let Some(at) = retry_at { schedule(&mut next, at); }
            let weather = !w.occluded && !w.rgb.open && !self.login.is_open() && (self.focused == Some(id) || self.test.drives_picker());
            if let Some(at) = w.room.next_frame(weather, !visible.is_empty() && w.preview_pending.load(Ordering::Acquire)) {
                if at <= now {
                    w.window.request_redraw();
                } else {
                    schedule(&mut next, at);
                }
            }
        }
        event_loop.set_control_flow(next.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

fn schedule(next: &mut Option<Instant>, at: Instant) {
    if next.is_none_or(|t| at < t) {
        *next = Some(at);
    }
}

/// Windows 새 판 칩 — 로그인 칩 아래. 누르면 설치기를 맡기고 끝난다(설치 뒤 다시 뜬다).
#[cfg(windows)]
fn paint_update(g: &mut GridRenderer, ui: &mut Ui, width: f32, version: &str) -> (f32, f32, f32, f32) {
    let text = format!("새 판 {version} · 눌러서 설치");
    let w = ui.measure(g, &text, 12.0, false) + 24.0;
    let r = (width - 28.0 - w, 60.0, w, 28.0);
    g.round_rect_fill(r.0, r.1, r.2, r.3, 6.0, [58, 128, 92, 255]);
    ui.draw(g, r.0 + 12.0, r.1 + 7.0, &text, 12.0, [244, 238, 228, 255], false);
    r
}
