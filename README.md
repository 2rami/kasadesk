# KasaDesk

다른 기기의 화면을 보는 작은 앱이에요. [카사텀](https://github.com/2rami/kasaterm)의 카사넷으로 그 기기의 화면 서비스에 바로 붙고,
카사텀과 따로 돌아서 이 기기나 그 기기의 카사텀이 다시 떠도 화면이 끊기지 않아요.

- 고르기 화면은 아이소메트릭 방 하나예요. 맥북은 책상 위, Windows 는 책상 위 모니터와 밑의 타워, 맥미니는 선반에 놓여요.
  기기를 누르면 카메라가 그 화면 앞으로 다가간 뒤 보기로 넘어가요.
- 기기 목록은 이 기기 카사텀에서 받고, 카사텀이 없는 컴퓨터에서는 계정으로 로그인해 받아요.
- 보내는 쪽 카사텀은 이 앱의 키를 「보기 전용」으로만 받아요 — 화면만 보이고 포트 돌리기는 못 열어요.

설계와 규약은 카사텀 문서 [`docs/screen-service-split.md`](https://github.com/2rami/kasaterm/blob/main/docs/screen-service-split.md)
「보기 앱 KasaDesk」에 있어요.

## 설치

[릴리스](https://github.com/2rami/kasadesk/releases/latest)에서 받아요.

- 맥: `KasaDesk-<판>-macos-arm64.dmg` — 앱을 응용 프로그램 폴더로 옮겨요(공증된 Developer ID 판).
- Windows: `KasaDesk-<판>-windows-x86_64.msi` — `Program Files\KasaDesk` 에 깔리고 시작 메뉴에 「KasaDesk」가 생겨요.

카사텀의 「화면 보기」는 깔린 KasaDesk 를 그 기기로 띄우고, 없으면 카사텀 안 화면 탭으로 열어요.

## 업데이트

카사텀과 같은 길이에요. 피드는 이 저장소의 Pages 에서 나가고, 판마다 이 앱 전용 EdDSA 키로 서명해요.

- 맥: 번들 안 Sparkle 이 [`appcast.xml`](https://2rami.github.io/kasadesk/appcast.xml)을 한 시간마다 보고, 새 판을 받아 두었다가 끌 때 갈아 끼워요.
- Windows: 앱이 [`appcast-win.xml`](https://2rami.github.io/kasadesk/appcast-win.xml)을 한 시간마다 보고, 새 판 MSI 를 받아 서명을
  확인한 뒤 고르기 화면에 「새 판」 칩을 띄워요. 칩을 누르거나 앱을 끄면 깔아요.

## 굽기

공용 크레이트(`kasa-castview`·`kasa-net`·`kasa-gridview` 등)는 카사텀 서브모듈(`kasaterm/`)의 고정 커밋에서 받아요.
카사텀 글꼴이 git-lfs 라 서브모듈에서 글꼴만 따로 받아야 해요.

```sh
git clone --recurse-submodules https://github.com/2rami/kasadesk
git -C kasaterm lfs pull --include="crates/kasa-gridview/assets/fonts/*"
cargo run                      # 개발 실행
./scripts/bundle-mac.sh        # dist/KasaDesk.app (서명 없으면 ad-hoc)
pwsh scripts/windows/package.ps1   # dist/KasaDesk-<판>-windows-x86_64.msi
```

카사텀 쪽 공용 크레이트를 바꿨으면 서브모듈을 그 커밋으로 올려요(`git -C kasaterm checkout <커밋>` 뒤 커밋).

격리 실행(리그)은 `KASATERM_VIEW_STATE`(상태 폴더)와 `KASATERM_VIEW_KEY`(보기 앱 키)를 줘요 — 그 실행은 업데이트하지 않아요.
리그 손잡이(`KASATERM_VIEW_TEST_*`)는 `src/rig.rs` 에 있어요.

## 릴리스

릴리스 기계(맥미니)에서 `python3 scripts/release.py` 하나로 내요 — 판 번호 올리기, 맥 굽기·공증, 태그·릴리스,
Windows 굽기(`.github/workflows/release.yml` 을 태그로 불러 MSI 를 붙임), 두 피드 서명·게시, Pages 확인까지.
서명 열쇠고리·공증 프로필·EdDSA 개인키는 그 기계에만 있고, 이 저장소와 GitHub 에는 비밀값이 없어요.

## 라이선스

MIT
