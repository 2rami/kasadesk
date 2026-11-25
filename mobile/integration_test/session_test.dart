import 'fixtures/video.dart';
import 'dart:async' show Timer;
import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:kasadesk_mobile/main.dart' as desk;
import 'package:kasadesk_mobile/room.dart';
import 'package:kasadesk_mobile/lighting.dart';

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'account room, color readback and remote input through an authenticated socket',
    (t) async {
      final gateway = Uri.parse(desk.gateway);
      expect(gateway.host, '127.0.0.1');
      final server = await HttpServer.bind(
        InternetAddress.loopbackIPv4,
        gateway.port,
      );
      await const FlutterSecureStorage().delete(key: desk.accountStorageKey);
      final colors = <String, String?>{};
      final inputs = <Map>[];
      final sockets = <WebSocket>[];
      final writes = <Map>[];
      var logouts = 0;
      const id = 'studio-pc';
      const jwt = 'local-test-session';
      final jpeg = base64Decode(
        '/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBUODAsLDBkSEw8VHhsgHx4bHR0hJTApISMtJB0dKjkqLTEzNjY2ICg7Pzo0PjA1NjP/2wBDAQkJCQwLDBgODhgzIh0iMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzMzP/wAARCAFoAoADASIAAhEBAxEB/8QAGwABAQEBAQEBAQAAAAAAAAAAAAcIBQYEAgP/xABBEAEAAAIHBQYFAgMHAwUAAAAAAxcBAgUGVKTSBFVlk+IHERI2dLIUIUWDwxMxFkFRFSIkQ2GBoTJCUmJxwcLh/8QAGQEBAQEBAQEAAAAAAAAAAAAAAAQDAgEF/8QAIREBAAICAQQDAQAAAAAAAAAAAAITAVEDERIxMgQUQSH/2gAMAwEAAhEDEQA/APAANnzgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB0LFsXbrftOHsGwQvHFr/OmtT8qtSr/ADrVqf5UUf8A5R3000UOe0NcW78GwLs7LV/R8G2bTDqxdprVqlNWvTWpo76KtNFPzo8NFPd3f6U091FNNLmcu3DTjh35eXs3sd2OrB77UtOPEi01av8Ad2WrRUoqU/8AdR31qKfFR/Snuo/b9vn8vtlBd/GWnzYehQBh3y2rqhpP5QXfxlp82HoJQXfxlp82HoUAO+WyqGk/lBd/GWnzYeglBd/GWnzYehQA75bKoaT+UF38ZafNh6CUF38ZafNh6FADvlsqhpP5QXfxlp82HoJQXfxlp82HoUAO+WyqGk/lBd/GWnzYeglBd/GWnzYehQA75bKoaT+UF38ZafNh6CUF38ZafNh6FADvlsqhpP5QXfxlp82HoJQXfxlp82HoUAO+WyqGk/lBd/GWnzYeglBd/GWnzYehQA75bKoaT+UF38ZafNh6CUF38ZafNh6FADvlsqhpP5QXfxlp82HoJQXfxlp82HoUAO+WyqGk/lBd/GWnzYeglBd/GWnzYehQA75bKoaT+UF38ZafNh6CUF38ZafNh6FADvlsqhpP5QXfxlp82HoJQXfxlp82HoUAO+WyqGk/lBd/GWnzYeglBd/GWnzYehQA75bKoaT+UF38ZafNh6CUF38ZafNh6FADvlsqhpP5QXfxlp82HoJQXfxlp82HoUAO+WyqGk/lBd/GWnzYeglBd/GWnzYehQA75bKoaT+UF38ZafNh6CUF38ZafNh6FADvlsqhpP5QXfxlp82HoJQXfxlp82HoUAO+WyqGk/lBd/GWnzYeglBd/GWnzYehQA75bKoaT+UF38ZafNh6CUF38ZafNh6FADvlsqhpP5QXfxlp82HoJQXfxlp82HoUAO+WyqGk/lBd/GWnzYeglBd/GWnzYehQA75bKoaT+UF38ZafNh6CUF38ZafNh6FADvlsqhpP5QXfxlp82HoJQXfxlp82HoUAO+WyqGk/lBd/GWnzYeglBd/GWnzYehQA75bKoaT+UF38ZafNh6CUF38ZafNh6FADvlsqhpP5QXfxlp82HoJQXfxlp82HoUAO+WyqGk/lBd/GWnzYeglBd/GWnzYehQA75bKoaT+UF38ZafNh6CUF38ZafNh6FADvlsqhpP5QXfxlp82HoJQXfxlp82HoUAO+WyqGk/lBd/GWnzYeglBd/GWnzYehQA75bKoaT+UF38ZafNh6Hz7Z2O2XXg0UbFae2QYvi+daPVqxaKaP6d1FFX5/t8+//ZSA75bKoaZsvFdm0rsbdV2a0KlWmivV8UONDppphxKP591NNFHzo/nRTR3/AO1NFNPHaYt+xNlvDY0ez9qq1aaK9WmmHErVe+mFX7v7tej50fOj/wB/nR30ftTSzVFhRIEavBjQ60OLUrU1a9SvR3U1aaPlTRTR/KltCXdhLy8fZn+PwA7ZAAAAAADVbKjVbHl/FPx/0B5y9d6/4Y+E/wAF8T8R4/8AN8Hh8Ph/0p7/APq/4ZKZSxHHXL0YnM1eC5roJq8FzXQdGd0NqMJzNXgua6CavBc10HQuhtRhOZq8FzXQTV4Lmug6F0NqMJzNXgua6CavBc10HQuhtRhOZq8FzXQTV4Lmug6F0NqMJzNXgua6CavBc10HQuhtRhOZq8FzXQTV4Lmug6F0NqMJzNXgua6CavBc10HQuhtRhOZq8FzXQTV4Lmug6F0NqMJzNXgua6CavBc10HQuhtRhOZq8FzXQTV4Lmug6F0NqMJzNXgua6CavBc10HQuhtRhOZq8FzXQTV4Lmug6F0NqMJzNXgua6CavBc10HQuhtRhxbsXg/iSzYm2fC/D+CNTC8H6nj7+6iinv7+6j+rtDTGcZx1wAD0AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAZpvR5ttn10b30tLM03o822z66N76WvF5T/I8YcoBskAAAAAAGq2VGq2PL+Kfj/onPar9J+9/9FGecvXdT+J/hP8b8N8P4/wDK8fi8Xh/1o7v+n/llhtyxzKGcYRUUeVXGsr1kquNZXrddUlM9JwKPKrjWV6yVXGsr1nUpnpOBR5VcayvWSq41les6lM9JwKPKrjWV6yVXGsr1nUpnpOBR5VcayvWSq41les6lM9JwKPKrjWV6yVXGsr1nUpnpOBR5VcayvWSq41les6lM9JwKPKrjWV6yVXGsr1nUpnpOBR5VcayvWSq41les6lM9JwKPKrjWV6yVXGsr1nUpnpOBR5VcayvWSq41les6lM9JwKPKrjWV6yVXGsr1nUpnpOBR5VcayvWSq41les6lM9JwKPKrjWV6yVXGsr1nUpnp0ezDy1tPrK3sqPauLdi7/wDDdmxNj+K+I8camL4/0/B3d9FFHd3d9P8AR2nOVnHjOI4xkAHYAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAzTejzbbPro3vpaWZpvR5ttn10b30teLyn+R4w5QDZIAAAAAANVsqNVseX8U/H/AEAZKgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABmm9Hm22fXRvfS0szTejzbbPro3vpa8XlP8jxhygGyQAAAAAAarZUarY8v4p+P+jy18r5fwl8F/gPi/ivH/nfp+Hw+H/009/f4v8Ah6lLO2T6L9/8biGMZl0y35M5xHOcE5OA5zoJycBznQlg3rjpLbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPapzk4DnOgnJwHOdCWBXHRbPa5XRv/8AxVa0XYf7M+F/TgUxfH+v4+/urVaO7u8NH/l/w9oi/ZF5r2r0Nf31FoYzxjGemFPFLMo9cjNN6PNts+uje+lpZmm9Hm22fXRvfS64vLP5HjDlANkgAAAAAA1Wyo1Wx5fxT8f9Es7ZPov3/wAappZ2yfRfv/jc8fthty+mUsAUogAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHvuyLzXtXoa/vqLQi/ZF5r2r0Nf31FoT8vsr4fUZpvR5ttn10b30tLM03o822z66N76XvF5c/I8YcoBskAAAAAAGq2VGq2PL+Kfj/AKJZ2yfRfv8A41TSztk+i/f/ABueP2w25fTKWAKUQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD33ZF5r2r0Nf31FoRfsi817V6Gv76i0J+X2V8PqM03o822z66N76Wlmab0ebbZ9dG99L3i8ufkeMOUA2SAAAAAADVbKjVbHl/FPx/0Sztk+i/f/GqaWdsn0X7/AONzx+2G3L6ZSwBSiAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAe+7IvNe1ehr++otCL9kXmvavQ1/fUWhPy+yvh9Rmm9Hm22fXRvfS0szTejzbbPro3vpe8Xlz8jxhygGyQAAAAAAarZUarY8v4p+P+iWdsn0X7/wCNU0s7ZPov3/xueP2w25fTKWAKUQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD33ZF5r2r0Nf31FoRfsi817V6Gv76i0J+X2V8PqM03o822z66N76Wlmab0ebbZ9dG99L3i8ufkeMOUA2SAAAAAADVbKjVbHl/FPx/0Sztk+i/f/GqaWdsn0X7/wCNzx+2G3L6ZSwBSiAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAe+7IvNe1ehr++otCL9kXmvavQ1/fUWhPy+yvh9Rmm9Hm22fXRvfS0szTejzbbPro3vpe8Xlz8jxhygGyQAAAAAAarZUarY8v4p+P+iWdsn0X7/41TSztk+i/f/G54/bDbl9MpYApRAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAPfdkXmvavQ1/fUWhF+yLzXtXoa/vqLQn5fZXw+ozTejzbbPro3vpaWZpvR5ttn10b30veLy5+R4w5QDZIAAAAAANVsqNVseX8U/H/AESztk+i/f8AxqmlnbJ9F+/+Nzx+2G3L6ZSwBSiAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAe+7IvNe1ehr++otCL9kXmvavQ1/fUWhPy+yvh9Rmm9Hm22fXRvfS0szTejzbbPro3vpe8Xlz8jxhygGyQAAAAAAarZUarY8v4p+P+iWdsn0X7/41TSztk+i/f8AxueP2w25fTKWAKUQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD33ZF5r2r0Nf31FoRfsi817V6Gv76i0J+X2V8PqM03o822z66N76Wlmab0ebbZ9dG99L3i8ufkeMOUA2SAAAAAADVbKjVbHl/FPx/0Sztk+i/f/ABqmlnbJ9F+/+Nzx+2G3L6ZSwBSiAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAe+7IvNe1ehr++otCL9kXmvavQ1/fUWhPy+yvh9Rmm9Hm22fXRvfS0szTejzbbPro3vpe8Xlz8jxhygGyQAAAAAAarZUarY8v4p+P8AolnbJ9F+/wDjVNLO2T6L9/8AG54/bDbl9MpYApRAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAPfdkXmvavQ1/fUWhF+yLzXtXoa/vqLQn5fZXw+ozTejzbbPro3vpaWZpvR5ttn10b30veLy5+R4w5QDZIAAAAAANVsqNVseX8U/H/RLO2T6L9/8appZ2yfRfv8A43PH7YbcvplLAFKIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB77si817V6Gv76i0Iv2Rea9q9DX99RaE/L7K+H1Gab0ebbZ9dG99LSzNN6PNts+uje+l7xeXPyPGHKAbJAAAAAABqtlRqtjy/in4/6PF3/ujaF6v7O+BjbND+G/U8f69atR3+Lw93d3UU/+NL2gzxnpnrhRKOJY6ZReUVv4yzebE0Eorfxlm82JoWgd2yZ0wReUVv4yzebE0Eorfxlm82JoWgLZFMEXlFb+Ms3mxNBKK38ZZvNiaFoC2RTBF5RW/jLN5sTQSit/GWbzYmhaAtkUwReUVv4yzebE0Eorfxlm82JoWgLZFMEXlFb+Ms3mxNBKK38ZZvNiaFoC2RTBF5RW/jLN5sTQSit/GWbzYmhaAtkUwReUVv4yzebE0Eorfxlm82JoWgLZFMEXlFb+Ms3mxNBKK38ZZvNiaFoC2RTBF5RW/jLN5sTQSit/GWbzYmhaAtkUwReUVv4yzebE0Eorfxlm82JoWgLZFMEXlFb+Ms3mxNBKK38ZZvNiaFoC2RTBF5RW/jLN5sTQSit/GWbzYmhaAtkUwReUVv4yzebE0Eorfxlm82JoWgLZFMEXlFb+Ms3mxNBKK38ZZvNiaFoC2RTBF5RW/jLN5sTQSit/GWbzYmhaAtkUwReUVv4yzebE0Eorfxlm82JoWgLZFMEXlFb+Ms3mxNBKK38ZZvNiaFoC2RTBF5RW/jLN5sTQSit/GWbzYmhaAtkUwReUVv4yzebE0Eorfxlm82JoWgLZFMEXlFb+Ms3mxNBKK38ZZvNiaFoC2RTBF5RW/jLN5sTQSit/GWbzYmhaAtkUwReUVv4yzebE0Eorfxlm82JoWgLZFMEXlFb+Ms3mxNBKK38ZZvNiaFoC2RTBF5RW/jLN5sTQSit/GWbzYmhaAtkUwReUVv4yzebE0Eorfxlm82JoWgLZFMEXlFb+Ms3mxNBKK38ZZvNiaFoC2RTBF5RW/jLN5sTQSit/GWbzYmhaAtkUwReUVv4yzebE0Eorfxlm82JoWgLZFMEXlFb+Ms3mxNBKK38ZZvNiaFoC2RTBF5RW/jLN5sTQSit/GWbzYmhaAtkUwReUVv4yzebE0Eorfxlm82JoWgLZFME+uLcW07sW3G23bY+xxIdfZq0KiiDXrU099NarT/OrR8v7tKgg4znOc9ctIxxHHTAzTejzbbPro3vpaWZpvR5ttn10b30tOLyx+R4w5QDZIAAAAAANVsqNIXStqrb92di239X9SN+nRU2imnw0U0RatHdW76KPlR30/Oij+lNHyoZcuPCn4+f7nDtgMVQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAzTejzbbPro3vpaLtO0dnsmzNptDaq3hg7PDpr1vnRRTT3ftRR300Ud9NPyoo/nTTQzJtW0xtt2uNtW0V/HGjRK0SJW7qKPFWpp76ae6j5fvS14sJvkZ8YfyAbJQAAAAAB27sXn2669p0bVstPjhV+6iPs9anuqxav8A8U0fyp/l/rRTTRTxAzjq9xnOM9cLnZvardzbIPftlePsEWirV8VWJCpr0U00/vRVpqd/fRR/Wmij96Pl/T7Zk3S3tlouln8Z1RbfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0kybpb2y0XSz+FWD7EmgJk3S3tloukmTdLe2Wi6WfwqwfYk0BMm6W9stF0vn2ztRuvs0GivB2mPtdamt3fpwIFaimij+v9/w0d3+/f80HCrB9iT0t7r6bdezaIdESp8NsUL5w9mq1/FR4u751q1PdR30/v3fL5Uf700+aBpjHTwxznOc9cgA8AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAf/2Q==',
      );
      server.listen((req) async {
        final path = req.uri.path;
        void reply(Object data, [int code = 200]) {
          req.response.statusCode = code;
          req.response.headers.contentType = ContentType.json;
          req.response.write(jsonEncode(data));
          req.response.close();
        }

        if (path == '/relay/oauth/providers') {
          reply({'providers': []});
          return;
        }
        if (path == '/relay/login') {
          final body = jsonDecode(await utf8.decoder.bind(req).join());
          if (body['account'] != 'studio' || body['password'] != 'fixture') {
            reply({}, 401);
            return;
          }
          reply({
            'account': 'studio',
            'device_id': 'fixture-phone',
            'token': jwt,
          });
          return;
        }
        if (path.endsWith('/screen/ws')) {
          if (!(req.headers.value('sec-websocket-protocol') ?? '').contains(
            'kasa-auth.$jwt',
          )) {
            reply({}, 401);
            return;
          }
          final ws = await WebSocketTransformer.upgrade(
            req,
            protocolSelector: (_) => 'kasa-relay-account',
          );
          sockets.add(ws);
          ws.add(
            jsonEncode({
              't': 'hello',
              if (req.uri.queryParameters['codec'] == 'hevc') 'codec': 'hevc',
              'display': 1,
              'displays': [
                {'id': 1, 'width': 640, 'height': 360, 'main': true},
                {'id': 2, 'width': 1280, 'height': 720, 'main': false},
              ],
            }),
          );
          if (req.uri.queryParameters['codec'] == 'hevc') {
            var seq = 0;
            final feed = Timer.periodic(const Duration(milliseconds: 70), (
              timer,
            ) {
              if (ws.readyState != WebSocket.open || seq >= 18) {
                timer.cancel();
                return;
              }
              final frame = base64Decode(hevcFrame);
              ByteData.sublistView(frame).setUint32(1, ++seq);
              ws.add(frame);
            });
            ws.done.whenComplete(feed.cancel);
          } else {
            final header = ByteData(6)..setUint32(0, 1);
            ws.add(
              Uint8List.fromList([...header.buffer.asUint8List(), ...jpeg]),
            );
          }
          ws.listen((event) {
            if (event is String) {
              inputs.add(jsonDecode(event));
            }
          }, onError: (_) {});
          return;
        }
        if (req.headers.value('authorization') != 'Bearer $jwt') {
          reply({}, 401);
          return;
        }
        if (path == '/relay/account-machines') {
          reply({
            'machines': [
              {'id': id, 'machine': 'Studio PC'},
              {'id': 'notebook-12', 'machine': 'MacBook Pro'},
            ],
          });
          return;
        }
        if (path.endsWith('/version')) {
          reply({'kasadesk_rgb': path.contains(id)});
          return;
        }
        if (path.endsWith('/kasadesk/rgb')) {
          final b = jsonDecode(await utf8.decoder.bind(req).join());
          if (b['op'] == 'capability') {
            reply({'v': 1, 'layout': 'y60'});
            return;
          }
          if (b['op'] == 'color') {
            writes.add(b);
            colors[b['spot']] = b['color'];
            reply({'applied': true});
            return;
          }
          reply({
            'connected': true,
            'gpu': 41,
            'pump': 2100,
            'liquid': 31.2,
            'fans': [
              for (final spot in spots.take(6))
                {
                  'spot': spot,
                  'mapped': true,
                  'fixed': colors[spot],
                  'outer': '83dccc',
                  'inner': 'aa9bdd',
                  'rpm': 895,
                },
            ],
            'gpu_light': {
              'controller': true,
              'sync': true,
              'asleep': false,
              'fixed': colors['gpu'],
              'sample': '80ceb9',
              'effect': 'ice',
              'utilization': 17,
            },
          });
          return;
        }
        if (path == '/relay/logout') {
          logouts++;
          reply({'ok': true});
          return;
        }
        reply({}, 404);
      });
      addTearDown(() async {
        for (final s in sockets) {
          await s.close();
        }
        await server.close(force: true);
      });
      Future<void> waitFor(bool Function() pred) async {
        for (var i = 0; i < 100 && !pred(); i++) {
          await t.pump(const Duration(milliseconds: 100));
          await Future<void>.delayed(const Duration(milliseconds: 50));
        }
        expect(pred(), isTrue);
      }

      bool converted = false;
      Future<List<int>> capture(String name) async {
        if (Platform.isAndroid && !converted) {
          converted = true;
          await binding.convertFlutterSurfaceToImage();
          await t.pumpAndSettle();
        }
        final bytes = await binding.takeScreenshot(name);
        await File(
          '${Directory.systemTemp.path}/kasadesk-$name.png',
        ).writeAsBytes(bytes);
        return bytes;
      }

      await t.pumpWidget(const desk.KasaDesk());
      await waitFor(() => find.byType(TextField).evaluate().length == 2);
      await t.enterText(find.byType(TextField).first, 'studio');
      await t.enterText(find.byType(TextField).last, 'fixture');
      await t.testTextInput.receiveAction(TextInputAction.done);
      await t.pumpAndSettle();
      await t.tap(find.widgetWithText(FilledButton, '로그인'));
      await waitFor(() => find.text('Studio PC').evaluate().isNotEmpty);
      await t.drag(find.byType(Room), const Offset(24, -12));
      await t.pumpAndSettle();
      await waitFor(() => sockets.isNotEmpty);
      await t.pump(const Duration(milliseconds: 300));
      await capture('room');
      await t.tap(find.byTooltip('본체 조명'));
      await waitFor(() => find.text('895 RPM').evaluate().isNotEmpty);
      await t.tap(find.text('NEPTUNE GPU'));
      await t.pumpAndSettle();
      await t.tap(find.byTooltip('#E66C70'));
      await t.pump();
      final scroll = find
          .descendant(
            of: find.byType(Lighting),
            matching: find.byType(Scrollable),
          )
          .first;
      await t.scrollUntilVisible(find.text('색 저장'), 140, scrollable: scroll);
      await t.tap(find.text('색 저장'));
      await waitFor(() => find.text('고정 색  #E66C70').evaluate().isNotEmpty);
      await capture('lighting');
      await t.scrollUntilVisible(
        find.widgetWithText(TextButton, '효과 따르기'),
        100,
        scrollable: scroll,
      );
      await t.tap(find.widgetWithText(TextButton, '효과 따르기'));
      await waitFor(() => writes.length == 2);
      expect(writes.map((m) => (m['spot'], m['color'])).toList(), [
        ('gpu', 'E66C70'),
        ('gpu', null),
      ]);
      Navigator.of(t.element(find.byType(Lighting))).pop();
      await t.pumpAndSettle();
      await t.tap(find.text('Studio PC'));
      await waitFor(() => find.byType(RawImage).evaluate().isNotEmpty);
      await t.tap(find.text('터치'));
      await t.pumpAndSettle();
      await t.tapAt(t.getCenter(find.byType(RawImage)));
      await waitFor(
        () => inputs.any((m) => m['t'] == 'pointer' && m['b'] == 1),
      );
      await t.tap(find.text('트랙패드'));
      await t.pumpAndSettle();
      await t.dragFrom(
        t.getCenter(find.byType(RawImage)),
        const Offset(25, 15),
      );
      await t.pump();
      await t.tap(find.byTooltip('키보드'));
      await t.pumpAndSettle();
      await t.enterText(find.byType(TextField), '테스트');
      await t.tap(find.byTooltip('전송'));
      await waitFor(
        () => inputs.any((m) => m['t'] == 'text' && m['s'] == '테스트'),
      );
      await t.pump(const Duration(milliseconds: 420));
      await capture('viewer');
      expect(inputs.any((m) => m['t'] == 'pointer' && m['b'] == 0), isTrue);
      await t.tap(find.byType(PopupMenuButton<String>));
      await t.pumpAndSettle();
      expect(find.text('고화질 영상 요청'), findsOneWidget);
      {
        await t.tap(
          find.widgetWithText(CheckedPopupMenuItem<String>, '고화질 영상 요청'),
        );
        await t.pumpAndSettle();
        await waitFor(() => find.byType(Texture).evaluate().isNotEmpty);
        await Future<void>.delayed(const Duration(milliseconds: 1250));
        final pixels = await capture('video');
        final codec = await ui.instantiateImageCodec(
          Uint8List.fromList(pixels),
        );
        final image = (await codec.getNextFrame()).image;
        final rgba = await image.toByteData(format: ui.ImageByteFormat.rawRgba);
        final offset =
            4 * ((image.height ~/ 2) * image.width + image.width ~/ 3);
        expect(rgba!.getUint8(offset + 1), inInclusiveRange(140, 175));
        expect(rgba.getUint8(offset), inInclusiveRange(85, 130));
        image.dispose();
        codec.dispose();
      }
      expect(logouts, 0);
      await t.pumpWidget(const SizedBox());
      await t.pump();
    },
  );
}
