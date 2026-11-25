import 'dart:async';
import 'dart:typed_data';
import 'dart:ui' as ui;
import 'package:kasadesk_protocol/screen_session.dart' show screenFrameHead;
import 'package:kasadesk_protocol/term_socket.dart';
import 'package:web_socket_channel/web_socket_channel.dart';
import 'connection.dart';

/// One small still per connection keeps an idle room from streaming four full-resolution desktops.
class RoomPreview {
  WebSocketChannel? _channel;
  int _generation = 0;
  void cancel() {
    _generation++;
    final old = _channel;
    _channel = null;
    unawaited(old?.sink.close().catchError((_) {}));
  }

  Future<ui.Image?> capture(DeskConnection link, Device device) async {
    cancel();
    final generation = _generation;
    final uri = link.server.wsUri(
      'screen/ws',
      machine: device.route,
      query: {'w': '480', 'h': '320', 'fps': '1', 'q': '45'},
    );
    final channel = connectTermSocket(
      uri,
      protocols: link.server.wsProtocolsFor(uri),
    );
    _channel = channel;
    try {
      final packet = await channel.stream
          .firstWhere(
            (e) => e is List<int> || (e is String && e.contains('"denied"')),
          )
          .timeout(const Duration(seconds: 5));
      if (generation != _generation ||
          packet is! List<int> ||
          packet.length > 500000) {
        return null;
      }
      final bytes = packet is Uint8List ? packet : Uint8List.fromList(packet);
      final head = screenFrameHead(bytes);
      if (head == null || head.$2 >= bytes.length) return null;
      final buffer = await ui.ImmutableBuffer.fromUint8List(
        Uint8List.sublistView(bytes, head.$2),
      );
      try {
        final desc = await ui.ImageDescriptor.encoded(buffer);
        try {
          if (desc.width > 4096 ||
              desc.height > 4096 ||
              desc.width * desc.height > 10000000) {
            return null;
          }
          final codec = await desc.instantiateCodec(
            targetWidth: desc.width.clamp(1, 480),
          );
          try {
            final image = (await codec.getNextFrame()).image;
            if (generation == _generation) return image;
            image.dispose();
          } finally {
            codec.dispose();
          }
        } finally {
          desc.dispose();
        }
      } finally {
        buffer.dispose();
      }
    } catch (_) {
      return null;
    } finally {
      if (identical(_channel, channel)) cancel();
    }
    return null;
  }
}
