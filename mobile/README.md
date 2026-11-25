# KasaDesk Mobile

iPhone·iPad·Android에서 같은 카사계정의 컴퓨터를 보고 조작하는 독립 앱이다. 집 PC에는 카사텀을 실행해 둔다. 기기 선택과 화면 소켓은 계정 관문을 사용한다.

- **방**에서 한 손가락으로 회전하고 두 손가락으로 확대한다. 한 번에 네 기기를 배치한다. 화면이나 기기 목록을 누르면 원격 보기가 열린다. 방 미리보기는 앱이 앞에 있을 때 한 화면씩 작은 장을 받아 표시한다. 기기는 모바일용 경량 모형으로 그린다.
- **보기**에서 핀치로 확대·이동한다. **터치**는 누른 위치, **트랙패드**는 손가락 이동량으로 포인터를 조작한다. 탭은 클릭, 길게 눌렀다 떼거나 두 손가락 탭은 우클릭, 두 손가락 이동은 스크롤이다. **드래그**를 켜면 손가락을 누른 채 항목을 끌 수 있다.
- **키보드**에서 한글을 조합해 전송한다. 키 줄로 Ctrl·Alt·Shift·Meta와 특수 키를 보낼 수 있다. 메뉴에서 모니터를 바꾸고 입력 표시를 끄거나 다시 연결할 수 있다. 기본 화면은 JPEG이며 **고화질 영상 요청**은 호환 호스트에서 HEVC를 요청한다. iOS는 VideoToolbox, Android는 MediaCodec으로 푼다.
- **본체 조명**에서 여섯 팬과 NEPTUNE GPU의 고정 색과 효과 대표색을 확인하고 색을 저장·해제한다. 냉각 탭에서 온도·RPM·Kraken LCD를 확인한다. RGB Control, 소유권을 확인하는 카사텀 호스트, 카사계정 연결이 필요하다. GPU 상태를 제공하는 Windows 호스트는 0.2.277부터다.
- 계정 토큰은 OS 보안 저장소에 저장한다. 프로필을 누르면 계정 정보가 열리고 로그아웃을 별도로 선택할 수 있다. 입력 피드백은 원격 동작 종류와 단축키를 일시 표시한다.

## 빌드 및 시험

Flutter 3.41.9와 Dart 3.11.5에서 검증했다. 저장소의 카사텀 서브모듈을 고정 커밋으로 초기화한 뒤 이 디렉터리에서 실행한다. `protocol/lib`와 모바일 자산은 저장소 상대 심볼릭 링크다. Windows에서 소스를 받아 빌드할 때는 Git 심볼릭 링크 지원이 필요하다.

```sh
flutter pub get
flutter analyze
flutter run
flutter build ios --release
```

iOS 번들 ID 및 Android 앱 ID는 `com.kasa.kasadesk.mobile`이다. iOS 실기기 설치에는 이 ID에 유효한 Apple 서명과 프로비저닝 프로파일이 필요하다. iOS 개발 서명 IPA는 프로파일에 등록된 기기에만 설치된다. Android ARM64 설치 파일은 [모바일 0.1.0 릴리스](https://github.com/2rami/kasadesk/releases/tag/mobile-v0.1.0)에서 받는다. Android 배포 빌드는 `KASADESK_ANDROID_KEYSTORE`, `KASADESK_ANDROID_PASSWORD`와 키 별칭 `kasadesk`를 사용한다. 이 값은 빌드 환경에서 공급한다.

```sh
flutter build apk --release --target-platform android-arm64
```

통합 시험은 전용 시뮬레이터·에뮬레이터 안에 루프백 관문과 화면 호스트를 세운다. 로그인을 거친 목록 조회, 3D 방, GPU 설정 왕복, 포인터·한글 전송 및 영상 디코딩을 확인한다.

```sh
KASADESK_QA_DIR=build/screenshots flutter drive -d DEVICE_ID \
  --driver=test_driver/capture.dart --target=integration_test/session_test.dart \
  --dart-define=DESK_GATEWAY=http://127.0.0.1:19673
```

조작 방식 참고: [RustDesk 모바일 조작](https://rustdesk.com/docs/en/client/android/), [Moonlight 터치·트랙패드 안내](https://github.com/moonlight-stream/moonlight-docs/wiki/Setup-Guide#keyboardmousegamepad-input-options).
