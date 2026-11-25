import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kasadesk_mobile/lighting.dart';
import 'package:kasadesk_mobile/viewer.dart';

void main() {
  test('long IME text preserves Unicode scalars across host input limits', () {
    final text = List.filled(900, '한글🌓').join();
    final chunks = remoteTextChunks(text).toList();
    expect(chunks.join(), text);
    expect(chunks.length, greaterThan(1));
    expect(chunks.every((c) => utf8.encode(c).length <= 4000), isTrue);
    expect(remoteTextChunks(''), isEmpty);
  });
  test(
    'GPU sleep, effect sample and fixed color are isolated from the fan sample',
    () {
      final state = <String, dynamic>{
        'connected': true,
        'fans': [
          {'spot': 'right1', 'outer': 'aabbcc'},
        ],
        'gpu_light': <String, Object?>{
          'controller': true,
          'sync': true,
          'asleep': true,
          'sample': '112233',
          'fixed': '445566',
        },
      };
      expect(RoomLights.from(state).gpu, Colors.black);
      expect(RoomLights.from(state).fans['right1'], parseHex('aabbcc'));
      state['gpu_light']['asleep'] = false;
      expect(RoomLights.from(state).gpu, parseHex('445566'));
      state['gpu_light']['fixed'] = null;
      expect(RoomLights.from(state).gpu, parseHex('112233'));
      state['gpu_light']['sync'] = false;
      expect(RoomLights.from(state).gpu, isNull);
      state.remove('gpu_light');
      expect(RoomLights.from(state).gpu, isNull);
    },
  );
}
