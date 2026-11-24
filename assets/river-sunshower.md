# 여우비 작업실

[강변 파노라마](river-panorama.jpg)는 2026년 11월 24일 OpenAI ImageGen으로 생성한 원본 배경화다. 소나기가 지나가는 가상의 강변 도시를 360° 정거원통 투영 이미지로 그렸다. 생성한 PNG를 JPEG로 변환해 배포한다. 방에서 이동하면 도시가 시차에 따라 움직이고, 먼 구름은 천천히 흘러간다.

창문의 빗줄기와 물방울, 창턱의 물보라는 KasaTerm의 작업 날씨 셰이더에서 가져왔다. 원본의 `grain`, `falling`, `bead`, `impact` 함수를 사용하고 물방울의 위치를 작업실 창문 좌표로 계산한다. 원본은 KasaTerm 작업 트리의 커밋 `e15221a26f7db23987a994a634f91f7aa7efe3ea`에 있는 `app/kasaterm/src/weather/shaders/showers.wgsl`이다. [KasaDesk에 포함한 셰이더](../src/scene/showers.wgsl)

배경과 셰이더는 [MIT 라이선스](../LICENSE)로 배포한다. KasaTerm 셰이더의 저작권은 © 2026 2rami에 있다.
