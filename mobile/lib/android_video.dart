import 'dart:async';
import 'package:flutter/services.dart';
import 'package:kasadesk_protocol/screen_video.dart';

/// Keep at most a few access units in flight; losing a reference frame requires another keyframe.
class AndroidVideo implements ScreenVideoSink {
  static const api = MethodChannel('kasadesk/android_video');
  int? _id;
  bool _closed = false, _waiting = true;
  int _pending = 0;
  @override
  void Function()? onNeedKeyframe;
  @override
  Future<int?> create() async {
    try {
      final id = await api.invokeMethod<int>('create');
      if (id == null) return null;
      if (_closed) {
        await api.invokeMethod('dispose', id);
        return null;
      }
      return _id = id;
    } on PlatformException {
      return null;
    } on MissingPluginException {
      return null;
    }
  }

  @override
  void reset() {
    _waiting = true;
    final id = _id;
    if (id != null) {
      unawaited(api.invokeMethod<void>('reset', id).catchError((_) {}));
    }
  }

  @override
  void push(Uint8List frame) {
    final id = _id, head = screenVideoHead(frame);
    if (_closed || id == null || head == null) return;
    if (_pending >= 4) {
      if (!_waiting) {
        _waiting = true;
        onNeedKeyframe?.call();
      }
      return;
    }
    if (_waiting && !head.key) return;
    _waiting = false;
    _pending++;
    unawaited(
      api
          .invokeMethod<bool>('frame', {'id': id, 'bytes': frame})
          .then((ok) {
            if (!_closed && ok != true) {
              _waiting = true;
              onNeedKeyframe?.call();
            }
          })
          .catchError((_) {
            if (!_closed) {
              _waiting = true;
              onNeedKeyframe?.call();
            }
          })
          .whenComplete(() => _pending--),
    );
  }

  @override
  void dispose() {
    _closed = true;
    final id = _id;
    _id = null;
    if (id != null) {
      unawaited(api.invokeMethod<void>('dispose', id).catchError((_) {}));
    }
  }
}
