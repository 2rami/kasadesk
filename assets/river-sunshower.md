# 여우비 작업실

[강변 파노라마](river-panorama.jpg)는 2026년 11월 24일 OpenAI ImageGen으로 생성한 원본 배경화다. 소나기가 지나가는 가상의 강변 도시를 360° 정거원통 투영 이미지로 그렸다. 생성한 PNG를 JPEG로 변환해 배포한다. 방에서 이동하면 도시가 시차에 따라 움직이고, 먼 구름은 천천히 흘러간다.

창문의 비와 물보라, 구름층, 눈·우박 입자와 돌풍은 KasaTerm 작업 날씨의 `falling`, `impact`, `bead`, `cloudBank`, `flakes`, `windScene`을 사용한다. 햇살·별·안개·번개도 같은 셰이더의 효과를 강변 파노라마 좌표에 맞췄다. [하늘 마스크](river-sky-mask.png)는 원본 강변 그림의 산 능선을 따라 만들었다. 원본은 KasaTerm 작업 트리의 커밋 `e15221a26f7db23987a994a634f91f7aa7efe3ea`에 있는 `app/kasaterm/src/weather/shaders/showers.wgsl`이다. [KasaDesk에 포함한 셰이더](../src/scene/showers.wgsl)

서울의 현재 날씨 코드와 풍속·풍향은 Open-Meteo 데이터를 사용한다. 데이터는 CC BY 4.0으로 제공된다. [Open-Meteo](https://open-meteo.com/) · [데이터 라이선스](https://creativecommons.org/licenses/by/4.0/)

배경과 셰이더는 [MIT 라이선스](../LICENSE)로 배포한다. KasaTerm 셰이더의 저작권은 © 2026 2rami에 있다.
