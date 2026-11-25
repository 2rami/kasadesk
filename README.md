# KasaDesk

다른 기기의 화면을 보는 작은 앱이에요. [카사텀](https://github.com/2rami/kasaterm)의 카사넷으로 그 기기의 화면 서비스에 바로 붙고,
카사텀과 따로 돌아서 이 기기나 그 기기의 카사텀이 다시 떠도 화면이 끊기지 않아요.

- 고르기 화면은 강변 도시를 내려다보는 방이에요. 창틀과 기기는 선명하게, 먼 강변은 부드럽게 보여요. 서울의 현재 날씨를 15분마다 받아 맑은 하늘, 구름, 안개, 비, 눈, 뇌우와 우박을 창밖에 그려요. 자작나무 책상에 맥북과 데스크톱을 놓고 맥미니마다 별도 책상과 Studio Display를 배치해요.
  WASD로 이동하고, 휠로 앞뒤를 오가고, 오른쪽 버튼을 누른 채 드래그해 둘러봐요. Home은 처음 시점으로 돌아가고 R은 날씨 움직임을 멈추거나 다시 재생해요. 기기에 포인터를 올리면 이름표와 받침 조명이 바뀌어요.
  날씨를 처음 받기 전에는 여우비 풍경을 보여요. 갱신에 실패하면 마지막으로 받은 날씨를 표시하고 2분 뒤 다시 확인해요. [서울 날씨 데이터 제공 · Open-Meteo](https://open-meteo.com/)
  방에 보이는 연결 가능한 기기는 각자의 화면을 동시에 보여요. 거리별로 15·24·30fps를 요청하고, 보이는 기기가 많거나 구형 송신기가 큰 원본을 보내면 부하에 맞춰 요청량을 낮춰요. 다른 페이지나 시야 밖의 화면은 전송을 쉬어요. 모형을 아직 받지 않은 기기도 연결되면 해당 자리에 실제 화면이 떠요. 미리보기는 각 모델의 액정에 맞춰 비율을 유지하고 넘치는 가장자리를 잘라요. 기기를 누르거나 Enter를 누르면 원격 보기로 들어가요.
  구형 Windows 화면 서비스가 원본 H.264 영상을 보내면 최대 3840×2160(세로 화면 포함)까지 받아 해당 미리보기 크기로 줄여요. 화면 기록 권한이 뒤늦게 허용되면 방에 보이는 기기는 거절 후 60초가 지난 다음 다시 연결해요.
- 원격 영상 위아래에 검은 여백이 있을 때, 영상 안에서 포인터를 위로 밀면 원격 화면의 첫 줄에, 아래로 밀면 마지막 줄에 붙어요. 원격 메뉴 막대나 독을 열고 다시 영상 쪽으로 움직여 항목을 선택해요. KasaDesk 상단 버튼을 쓰려면 해당 버튼 아래에서 영상 상단에 닿은 뒤 계속 위로 미세요. **조작 중**을 눌러 보기 모드로 바꾸면 여백도 자유롭게 오갈 수 있어요.
- 오른쪽 위 계정 버튼에서 표시 이름·계정 ID와 현재 목록의 계정 기기 수를 확인해요. 로그아웃은 계정 메뉴에서 따로 선택해요. 오른쪽 아래 **조작법**에서 이동 키를 확인할 수 있어요.
- 기기 목록은 이 기기 카사텀에서 받고, 카사텀이 없는 컴퓨터에서는 계정으로 로그인해 받아요.
- 보내는 쪽 카사텀은 이 앱의 키를 「보기 전용」으로만 받아요 — 화면만 보이고 포트 돌리기는 못 열어요.

설계와 규약은 카사텀 문서 [`docs/screen-service-split.md`](https://github.com/2rami/kasaterm/blob/main/docs/screen-service-split.md)
「보기 앱 KasaDesk」에 있어요.

## 설치

[릴리스](https://github.com/2rami/kasadesk/releases/latest)에서 받아요.

- 맥: `KasaDesk-<판>-macos-arm64.dmg` — 앱을 응용 프로그램 폴더로 옮겨요(공증된 Developer ID 판).
- Windows: `KasaDesk-<판>-windows-x86_64.msi` — `Program Files\KasaDesk` 에 깔리고 시작 메뉴에 「KasaDesk」가 생겨요.

카사텀의 「화면 보기」는 깔린 KasaDesk 를 그 기기로 띄우고, 없으면 카사텀 안 화면 탭으로 열어요.

## 모바일

iPhone·iPad·Android용 독립 앱에서 3D 방과 기기 목록, 원격 화면 보기·직접 터치·트랙패드, GPU와 팬 조명을 사용할 수 있어요. 모바일 설치와 빌드는 [KasaDesk Mobile](mobile/README.md)에 정리했어요. 데스크톱 원격 보기에서도 **입력 표시**로 클릭·드래그·스크롤·단축키 피드백을 켜고 끌 수 있어요.

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

## Y60 조명과 Kraken LCD

Windows PC에서 카사텀에 로그인하고 RGB Control을 실행해요. 다른 기기의 KasaDesk에서 같은 카사계정으로 로그인하면
계정 명부에서 여섯 팬이 배치된 Y60을 찾아요. 이 연결에는 RGB를 지원하는 카사텀 호스트·관문·KasaDesk가 필요해요.

방에서 **본체나 본체 이름표**를 누르면 조명판이 열려요. 팬을 직접 누르면 그 팬이 선택돼요.
**모니터나 기기 이름표**를 누르면 원격 화면이 열려요. 조명판은 오른쪽 아래 **조명 · LCD** 또는 키보드 **L**로도 열 수 있어요.
색상표나 6자리 HEX를 고른 뒤 **색 적용**을 누르면 실물 팬에 전달돼요. **효과 따르기**는 그 자리의 고정 색과 링별 색을 해제해요.
**그래픽카드** 탭을 열거나 본체 안의 NEPTUNE 카드를 누르면 GPU 온도·사용률, 고정 색 또는 효과 대표색, 조명 연결 상태가 보여요.
색상·밝기 막대를 드래그하거나 색상표·HEX로 고르면 카드의 두 LED 줄에서 미리 볼 수 있어요. **GPU 색 저장**은 그 설정을
집 PC에 저장하고 **효과 따르기**는 GPU 고정 색을 해제해요. 효과 대표색은 RGB Control이 제공하는 팬 채널 견본에서 계산해요.
모니터 연동 소등과 장치 동기화 상태에 따라 저장된 색과 출력은 달라질 수 있어요.
GPU 제어를 지원하는 계정 호스트가 연결돼야 저장할 수 있어요. 기존 팬 전용 연결에서도 색을 미리 볼 수 있어요.

**냉각 · LCD**에는 Kraken LCD 그림, 수온, 펌프와 선택한 팬 포트의 RPM, GPU 온도가 보여요. Tab과 Enter로 조작하고 Esc로 방에 돌아와요.

SSH로 페어링한 기존 PC도 계속 사용할 수 있어요. 이 방식은 호스트에서 `python kasadesk_bridge.py install`을 실행하고
KasaDesk에서 `python3 scripts/pair-rgb.py SSH별칭 카사넷기기ID`로 연결한 뒤 위의 기기 매핑에서 `case`에 `hyte-y60`을 지정해요.
브리지 토큰은 로컬 `KasaDesk/rgb.json`에 저장돼요. 격리 실행에서는 `KASADESK_RGB_CONFIG`에 지정한 SSH 설정만 읽어요.
[RGB Control SSH 연결과 권한 회수](https://github.com/2rami/rgb-control/blob/master/KasaDesk.md)

## 업데이트

카사텀과 같은 길이에요. 피드는 이 저장소의 Pages 에서 나가고, 판마다 이 앱 전용 EdDSA 키로 서명해요.

- 맥: 번들 안 Sparkle 이 [`appcast.xml`](https://2rami.github.io/kasadesk/appcast.xml)을 한 시간마다 보고, 새 판을 받아 두었다가 끌 때 갈아 끼워요. 앱 메뉴 「업데이트 확인…」을 누르면 끌 때까지 기다리지 않고 지금 받아 설치해요.
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

맥 릴리스 기계(개발 맥이 먼저, 맥미니가 예비)에서 `python3 scripts/release.py` 하나로 내요 — 판 번호 올리기, 맥 굽기·공증,
태그·릴리스, Windows 굽기(`.github/workflows/release.yml` 을 태그로 불러 집 PC 자체 러너가 MSI 를 붙임), 두 피드 서명·게시,
Pages 확인까지. 한 번에 한 기계에서만 돌려요.
Developer ID 열쇠고리·공증 프로필·EdDSA 개인키는 릴리스 기계에만 있고(EdDSA 키는 집 PC 에도 같은 것), 이 저장소와 GitHub 에는
비밀값이 없어요.

## 라이선스

[MIT 라이선스](LICENSE). 강변 배경과 창문 날씨 효과의 출처는 [강변 배경 제작 기록](assets/river-sunshower.md)에 있어요.
