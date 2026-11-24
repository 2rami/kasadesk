//! 자동 업데이트 — 카사텀과 같은 길. 맥은 번들 안 Sparkle 이 `Info.plist` 의 피드(`SUFeedURL`)를 보고 받아 두었다가 끌 때
//! 갈아 끼운다. 앱 메뉴 「업데이트 확인…」은 끌 때까지 기다리지 않고 지금 묻는다. Windows 는 이 파일이 피드(`appcast-win.xml`)를 한 시간마다 보고, 새 판 MSI 를 받아 EdDSA 서명을 확인한 뒤
//! 고르기 화면에 「새 판」 칩을 띄운다 — 누르거나 앱을 끄면 설치한다.
//! 격리 실행(`KASATERM_VIEW_STATE`·`KASATERM_VIEW_KEY`)과 설치본이 아닌 실행(cargo 산출물)은 업데이트하지 않는다.

/// 피드 서명 공개키(Sparkle EdDSA, 맥 번들의 `SUPublicEDKey` 와 같다). 개인키는 릴리스 기계에만 있다 — `scripts/release.py`.
#[cfg(any(windows, test))]
pub const ED_PUBLIC_KEY: &str = "LEspOJR62N02A/l32YWcZYT24qrbP8urZxOTAyWeQ/I=";
#[cfg(windows)]
pub const WIN_FEED: &str = "https://2rami.github.io/kasadesk/appcast-win.xml";

fn isolated() -> bool {
    ["KASATERM_VIEW_STATE", "KASATERM_VIEW_KEY"].iter().any(|k| std::env::var_os(k).is_some_and(|v| !v.is_empty()))
}

#[cfg(target_os = "macos")]
pub fn start() {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool};

    let Some(exe) = std::env::current_exe().ok() else { return };
    let Some(contents) = exe.parent().and_then(|p| p.parent()).filter(|c| c.join("Info.plist").is_file()) else { return };
    if isolated() {
        return;
    }
    let framework = contents.join("Frameworks/Sparkle.framework/Versions/B/Sparkle");
    let Ok(path) = std::ffi::CString::new(framework.to_string_lossy().as_bytes()) else { return };
    // SAFETY: 번들 안 Sparkle 프레임워크를 한 번 싣는다 — 닫지 않는다.
    if unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW) }.is_null() {
        eprintln!("[kasadesk] Sparkle 을 못 싣는다: {}", framework.display());
        return;
    }
    let Some(class) = AnyClass::get(c"SPUStandardUpdaterController") else {
        eprintln!("[kasadesk] SPUStandardUpdaterController 가 없다");
        return;
    };
    let none: *mut AnyObject = std::ptr::null_mut();
    // SAFETY: 표준 업데이터 컨트롤러를 만들어 앱이 끝날 때까지 쥔다(놓지 않는다). 점검 간격·자동 설치는 Info.plist 가 정한다.
    let controller: *mut AnyObject = unsafe {
        let made: *mut AnyObject = msg_send![class, alloc];
        msg_send![made, initWithStartingUpdater: Bool::YES, updaterDelegate: none, userDriverDelegate: none]
    };
    if controller.is_null() {
        eprintln!("[kasadesk] Sparkle 업데이터를 못 세운다");
        return;
    }
    eprintln!("[kasadesk] Sparkle 업데이터 시작");
    add_check_item(controller);
}

/// 앱 메뉴의 「KasaDesk에 관하여」 바로 아래에 「업데이트 확인…」을 단다. 컨트롤러의 `checkForUpdates:` 로 곧장 이어서
/// 받아 둔 판이 있으면 지금 설치·재시작을 묻고, 없으면 피드를 보고 「최신」까지 Sparkle 이 안내한다.
#[cfg(target_os = "macos")]
fn add_check_item(controller: *mut objc2::runtime::AnyObject) {
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::{msg_send, sel};

    let (Some(app_class), Some(item_class), Some(string)) =
        (AnyClass::get(c"NSApplication"), AnyClass::get(c"NSMenuItem"), AnyClass::get(c"NSString"))
    else {
        return;
    };
    // SAFETY: 주 스레드(winit 의 resumed)에서 winit 이 세운 기본 메뉴에 항목 하나를 끼운다. 항목은 메뉴가 쥐고, 표적인
    // 컨트롤러는 앱이 끝날 때까지 놓지 않는다.
    unsafe {
        let app: *mut AnyObject = msg_send![app_class, sharedApplication];
        let main: *mut AnyObject = msg_send![app, mainMenu];
        if main.is_null() {
            return;
        }
        let count: isize = msg_send![main, numberOfItems];
        if count < 1 {
            return;
        }
        let first: *mut AnyObject = msg_send![main, itemAtIndex: 0isize];
        let menu: *mut AnyObject = msg_send![first, submenu];
        if menu.is_null() {
            return;
        }
        let title: *mut AnyObject = msg_send![string, stringWithUTF8String: c"업데이트 확인…".as_ptr()];
        let key: *mut AnyObject = msg_send![string, stringWithUTF8String: c"".as_ptr()];
        let made: *mut AnyObject = msg_send![item_class, alloc];
        let item: *mut AnyObject = msg_send![made, initWithTitle: title, action: sel!(checkForUpdates:), keyEquivalent: key];
        let _: () = msg_send![item, setTarget: controller];
        let at: isize = msg_send![menu, numberOfItems];
        let _: () = msg_send![menu, insertItem: item, atIndex: at.min(1)];
    }
}

/// 피드의 맨 앞 항목 — 판 번호·내려받을 주소·EdDSA 서명·길이.
#[cfg(any(windows, test))]
#[derive(Debug, PartialEq, Eq)]
pub struct Item {
    pub version: String,
    pub url: String,
    pub signature: String,
    pub length: u64,
}

#[cfg(any(windows, test))]
pub fn parse_item(xml: &str) -> Option<Item> {
    let item = &xml[xml.find("<item>")?..];
    let item = &item[..item.find("</item>")?];
    let element = |name: &str| -> Option<String> {
        let open = format!("<{name}>");
        let start = item.find(&open)? + open.len();
        let rest = &item[start..];
        Some(rest[..rest.find('<')?].trim().to_string())
    };
    let enclosure = &item[item.find("<enclosure")?..];
    let enclosure = &enclosure[..enclosure.find('>')?];
    let attribute = |name: &str| -> Option<String> {
        let key = format!("{name}=\"");
        let start = enclosure.find(&key)? + key.len();
        let rest = &enclosure[start..];
        Some(rest[..rest.find('"')?].to_string())
    };
    let version = element("sparkle:version").or_else(|| attribute("sparkle:version"))?;
    Some(Item {
        version,
        url: attribute("url")?,
        signature: attribute("sparkle:edSignature")?,
        length: attribute("length")?.parse().ok()?,
    })
}

/// 점으로 가른 숫자끼리 비교한다. 숫자가 아닌 조각이 있으면 새 판으로 보지 않는다.
#[cfg(any(windows, test))]
pub fn newer(candidate: &str, current: &str) -> bool {
    let parse = |s: &str| -> Option<Vec<u64>> { s.split('.').map(|p| p.parse().ok()).collect() };
    matches!((parse(candidate), parse(current)), (Some(a), Some(b)) if a > b)
}

#[cfg(any(windows, test))]
pub fn verify(bytes: &[u8], signature_b64: &str, public_b64: &str) -> bool {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;
    let (Ok(signature), Ok(public)) = (b64.decode(signature_b64.trim()), b64.decode(public_b64)) else { return false };
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public).verify(bytes, &signature).is_ok()
}

#[cfg(windows)]
pub use windows::{install_now, start};

#[cfg(windows)]
mod windows {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::Duration;

    use super::{isolated, newer, parse_item, verify, ED_PUBLIC_KEY, WIN_FEED};

    const FIRST_CHECK: Duration = Duration::from_secs(15);
    const EVERY: Duration = Duration::from_secs(3600);

    /// 받아서 서명까지 확인한 새 판.
    #[derive(Clone, Debug)]
    pub struct Ready {
        pub version: String,
        msi: PathBuf,
    }

    static READY: Mutex<Option<Ready>> = Mutex::new(None);

    /// 설치본(Program Files\KasaDesk)에서 돌 때만 피드를 본다. 받은 새 판은 `notify` 로 알린다.
    pub fn start(handle: tokio::runtime::Handle, state: PathBuf, notify: impl Fn(String) + Send + 'static) {
        let installed = std::env::current_exe().ok().is_some_and(|exe| {
            exe.parent().and_then(Path::file_name).is_some_and(|dir| dir.eq_ignore_ascii_case("KasaDesk"))
        });
        if !installed || isolated() {
            return;
        }
        handle.spawn(async move {
            tokio::time::sleep(FIRST_CHECK).await;
            loop {
                match check(&state).await {
                    Ok(Some(ready)) => {
                        let version = ready.version.clone();
                        *READY.lock().unwrap_or_else(|e| e.into_inner()) = Some(ready);
                        notify(version);
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("[kasadesk] 업데이트 확인 실패: {e}"),
                }
                tokio::time::sleep(EVERY).await;
            }
        });
    }

    async fn check(state: &Path) -> Result<Option<Ready>, String> {
        let client = reqwest::Client::builder().timeout(Duration::from_secs(60)).build().map_err(|e| e.to_string())?;
        let xml = client.get(WIN_FEED).send().await.and_then(|r| r.error_for_status()).map_err(|e| e.to_string())?
            .text().await.map_err(|e| e.to_string())?;
        let Some(item) = parse_item(&xml) else { return Err("피드에 항목이 없다".into()) };
        if !newer(&item.version, env!("CARGO_PKG_VERSION")) {
            return Ok(None);
        }
        let dir = state.join("update");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let msi = dir.join(format!("KasaDesk-{}.msi", item.version));
        if !std::fs::read(&msi).is_ok_and(|b| b.len() as u64 == item.length && verify(&b, &item.signature, ED_PUBLIC_KEY)) {
            let bytes = client.get(&item.url).send().await.and_then(|r| r.error_for_status()).map_err(|e| e.to_string())?
                .bytes().await.map_err(|e| e.to_string())?;
            if bytes.len() as u64 != item.length || !verify(&bytes, &item.signature, ED_PUBLIC_KEY) {
                return Err(format!("{} 의 길이나 서명이 피드와 다르다 — 버린다", item.version));
            }
            std::fs::write(&msi, &bytes).map_err(|e| e.to_string())?;
        }
        eprintln!("[kasadesk] 새 판 {} 준비 — {}", item.version, msi.display());
        Ok(Some(Ready { version: item.version, msi }))
    }

    /// 받아 둔 새 판을 설치한다 — 설치기가 이 실행 파일을 갈아 끼울 수 있게 부른 쪽은 바로 끝나야 한다. `relaunch` 면 다 깐 뒤 다시 띄운다.
    pub fn install_now(relaunch: bool) -> bool {
        use std::os::windows::process::CommandExt as _;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        let Some(ready) = READY.lock().unwrap_or_else(|e| e.into_inner()).take() else { return false };
        let exe = std::env::current_exe().ok();
        let mut line = format!("msiexec /i \"{}\" /passive /norestart", ready.msi.display());
        if let Some(exe) = exe.filter(|_| relaunch) {
            line = format!("{line} && start \"\" \"{}\"", exe.display());
        }
        let spawned = std::process::Command::new("cmd")
            .args(["/d", "/c"])
            .raw_arg(&line)
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn();
        match spawned {
            Ok(_) => {
                eprintln!("[kasadesk] {} 설치를 맡겼다", ready.version);
                true
            }
            Err(e) => {
                eprintln!("[kasadesk] 설치기를 못 띄운다: {e}");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"<?xml version="1.0"?><rss><channel><title>KasaDesk</title>
        <item><title>0.1.2</title><sparkle:version>0.1.2</sparkle:version>
        <enclosure url="https://github.com/2rami/kasadesk/releases/download/v0.1.2/KasaDesk-0.1.2-windows-x86_64.msi"
            sparkle:os="windows" sparkle:edSignature="c2ln" length="4096" type="application/octet-stream"/></item>
        <item><title>0.1.1</title><sparkle:version>0.1.1</sparkle:version></item></channel></rss>"#;

    #[test]
    fn reads_the_newest_item_of_a_feed() {
        let item = parse_item(FEED).unwrap();
        assert_eq!(item, Item {
            version: "0.1.2".into(),
            url: "https://github.com/2rami/kasadesk/releases/download/v0.1.2/KasaDesk-0.1.2-windows-x86_64.msi".into(),
            signature: "c2ln".into(),
            length: 4096,
        });
        assert!(parse_item("<rss></rss>").is_none());
    }

    #[test]
    fn versions_compare_by_number() {
        assert!(newer("0.1.10", "0.1.9"));
        assert!(newer("0.2.0", "0.1.99"));
        assert!(!newer("0.1.9", "0.1.9"));
        assert!(!newer("0.2.0-beta", "0.1.0"));
    }

    #[test]
    fn a_signature_only_passes_with_the_matching_key_and_bytes() {
        use base64::Engine as _;
        use ring::signature::KeyPair as _;
        let b64 = base64::engine::general_purpose::STANDARD;
        let pair = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let public = b64.encode(pair.public_key().as_ref());
        let signature = b64.encode(pair.sign(b"msi").as_ref());
        assert!(verify(b"msi", &signature, &public));
        assert!(!verify(b"msj", &signature, &public));
        assert!(!verify(b"msi", &signature, ED_PUBLIC_KEY));
    }
}
