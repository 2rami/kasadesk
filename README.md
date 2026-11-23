# KasaDesk

다른 기기의 화면을 보는 작은 앱이에요. [카사텀](https://github.com/2rami/kasaterm)의 카사넷으로 그 기기의 화면 서비스에 바로 붙고,
카사텀과 따로 돌아서 이 기기나 그 기기의 카사텀이 다시 떠도 화면이 끊기지 않아요.

- 고르기 화면은 행성의 지평선을 바라보는 궤도 관측실이에요. 월넛 책상에 맥북과 데스크톱을 놓고 맥미니마다 별도 책상과 Studio Display를 배치해요.
  WASD로 이동하고, 휠로 앞뒤를 오가고, 오른쪽 버튼을 누른 채 드래그해 둘러봐요. Home은 처음 시점으로 돌아가요.
  연결 가능한 기기를 가리키거나 방향키로 고르면 해당 모니터에 저해상도 화면 미리보기가 떠요. 기기를 누르거나 Enter를 누르면 보기로 들어가요.
- 기기 목록은 이 기기 카사텀에서 받고, 카사텀이 없는 컴퓨터에서는 계정으로 로그인해 받아요.
- 보내는 쪽 카사텀은 이 앱의 키를 「보기 전용」으로만 받아요 — 화면만 보이고 포트 돌리기는 못 열어요.

설계와 규약은 카사텀 문서 [`docs/screen-service-split.md`](https://github.com/2rami/kasaterm/blob/main/docs/screen-service-split.md)
「보기 앱 KasaDesk」에 있어요.

## 설치

[릴리스](https://github.com/2rami/kasadesk/releases/latest)에서 받아요.

- 맥: `KasaDesk-<판>-macos-arm64.dmg` — 앱을 응용 프로그램 폴더로 옮겨요(공증된 Developer ID 판).
- Windows: `KasaDesk-<판>-windows-x86_64.msi` — `Program Files\KasaDesk` 에 깔리고 시작 메뉴에 「KasaDesk」가 생겨요.

카사텀의 「화면 보기」는 깔린 KasaDesk 를 그 기기로 띄우고, 없으면 카사텀 안 화면 탭으로 열어요.

## 제조사 기기 모델

Apple과 Dell의 제품 페이지에서 제공하는 3D 모델을 이 컴퓨터의 모델 폴더에 준비할 수 있어요.
Python 3과 Blender 4.4 이상이 설치된 환경에서 저장소 루트의 다음 명령을 실행하고 KasaDesk를 열어요.

```sh
python3 scripts/prepare-models.py
```

스크립트는 [제품 카탈로그](scripts/official-models.json)의 원본 파일을 받아 SHA-256을 확인하고 메시와 텍스처를 변환해요.
Blender의 실행 경로는 `--blender`로 지정할 수 있어요. 모델 폴더는 맥에서 `~/Library/Application Support/KasaDesk/Models`,
Windows에서 `%LOCALAPPDATA%/KasaDesk/Models`예요. `KASADESK_MODEL_DIR`을 지정하면 앱과 준비 스크립트가 그 폴더를 써요.

앱은 기기가 보고한 모델 식별자로 제품을 찾아요. 조립 PC처럼 식별자만으로 제품을 알 수 없으면 모델 폴더의
`bindings.json`에 카사넷 기기 ID 또는 명부 이름으로 연결할 수 있어요. 예를 들어 아래 설정은 한 PC에 Dell 모니터와 HYTE 케이스를 연결해요.
`hyte-y60`은 제조사 치수와 제품 사진을 참고해 KasaDesk에서 직접 만든 Snow White 모형이에요. 유리 세 면을 통해 보이는
부품도 직접 모델링했어요. NZXT N7 Z790에 G.SKILL Trident Z5 RGB 두 개를 꽂고, Kraken Elite 360 RGB White(2023)의
원형 LCD와 상단 360mm 라디에이터를 연결했어요. 수직으로 세운 Colorful iGame RTX 4070 Ti Neptune은 측면 240mm 라디에이터를 써요.
화이트 UNI FAN SL-INF 120은 상단 세 개, 측면 두 개, 후면 한 개예요. 호스 경로와 조명색은 장면에 맞춘 구성이에요.
[HYTE Y60](https://hyte.com/en-GB/store/y60/cs-hyte-y60-ww) · [N7 Z790](https://support.nzxt.com/hc/en-us/articles/37693799549211-N7-Z790-Specs) · [Kraken Elite](https://support.nzxt.com/hc/en-us/articles/47357851031579-Kraken-Elite-2023-Specs) · [Neptune](https://www.colorful.cn/home/product?id=2009&mid=102) · [SL-INF](https://lian-li.com/product/uni-fan-sl-infinity/)

다른 모형이 준비되기 전에도 이름표로 기기에 들어갈 수 있어요.

```json
{
  "작업용 PC": { "display": "aw3821dw", "case": "hyte-y60" },
  "작업용 맥북": { "computer": "m4" }
}
```

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

[MIT 라이선스](LICENSE). 관측실 배경의 제작 출처와 라이선스는 [orbit.jpg 설명](assets/orbit.jpg.md)에 있어요.
