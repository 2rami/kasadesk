import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:kasadesk_protocol/relay_account.dart';
import 'package:kasadesk_protocol/server.dart';

/// Use the app's own label in the account's device list while retaining the shared login flow.
class DeskLoginClient extends http.BaseClient {
  DeskLoginClient(this.origin, this.installId);
  final Uri origin;
  final Future<String> Function() installId;
  final http.Client _inner = http.Client();
  @override
  Future<http.StreamedResponse> send(http.BaseRequest request) async {
    if (request is http.Request &&
        request.method == 'POST' &&
        sameOrigin(origin, request.url) &&
        const {
          '/relay/login',
          '/relay/oauth/start',
        }.contains(request.url.path)) {
      final data = jsonDecode(request.body) as Map<String, dynamic>;
      data['label'] = 'KasaDesk Mobile';
      data['machine_id'] ??= await installId();
      request.body = jsonEncode(data);
    }
    return _inner.send(request);
  }

  @override
  void close() => _inner.close();
}

enum Shape { notebook, mini, desktop, unknown }

class Device {
  const Device(
    this.id,
    this.name, {
    this.shape = Shape.unknown,
    this.lights = false,
  });
  final String id, name;
  final Shape shape;
  final bool lights;
  String get route => '~$id';
}

class DeskConnection {
  DeskConnection(this.session, {http.Client? client})
    : server = Server.account(session, connectDirect: false),
      _http = OriginClient(
        session.origin,
        client: client,
        token: session.token,
      );
  final AccountSession session;
  final Server server;
  final OriginClient _http;
  final _id = RegExp(r'^[A-Za-z0-9_.-]{8,128}$');
  void Function()? expired;
  bool _closed = false;

  Future<Map<String, dynamic>> _request(
    String path, [
    Map<String, Object?>? body,
  ]) async {
    final uri = session.origin.resolve('/relay/$path');
    final req = http.Request(body == null ? 'GET' : 'POST', uri);
    if (body != null) {
      req.headers['content-type'] = 'application/json';
      req.body = jsonEncode(body);
    }
    final res = await _http.send(req).timeout(const Duration(seconds: 12));
    if (res.statusCode == 401 && !_closed) {
      expired?.call();
    }
    if (res.statusCode ~/ 100 != 2) {
      await res.stream.drain<void>().timeout(const Duration(seconds: 5));
      throw AccountException(
        accountError(res.statusCode),
        status: res.statusCode,
      );
    }
    final bytes = <int>[];
    await for (final chunk in res.stream.timeout(const Duration(seconds: 12))) {
      if (bytes.length + chunk.length > 420000) {
        throw const FormatException('응답이 너무 커요');
      }
      bytes.addAll(chunk);
    }
    return jsonDecode(utf8.decode(bytes)) as Map<String, dynamic>;
  }

  Future<List<Device>> devices() async {
    final list = (await _request('account-machines'))['machines'] as List;
    final rows = <Device>[
      for (final m in list.whereType<Map>())
        if (m['id'] is String &&
            _id.hasMatch(m['id']) &&
            m['machine'] is String)
          Device(m['id'], m['machine']),
    ];
    final result = <Device>[];
    for (var i = 0; i < rows.length; i += 4) {
      result.addAll(await Future.wait(rows.skip(i).take(4).map(_describe)));
    }
    return result;
  }

  Future<Device> _describe(Device d) async {
    try {
      final v = await _request('account/m/${d.route}/version');
      var lights = false;
      if (v['kasadesk_rgb'] == true) {
        try {
          final c = await rgb(d, {'op': 'capability'});
          lights = c['v'] == 1 && c['layout'] == 'y60';
        } catch (_) {}
      }
      final label = '${v['model'] ?? ''} ${d.name}'.toLowerCase();
      final shape = lights
          ? Shape.desktop
          : label.contains('mini') || label.contains('미니')
          ? Shape.mini
          : label.contains('book') || label.contains('맥북')
          ? Shape.notebook
          : label.contains('desktop') ||
                label.contains('데스크') ||
                label.contains('pc')
          ? Shape.desktop
          : Shape.unknown;
      return Device(d.id, d.name, shape: shape, lights: lights);
    } catch (_) {
      return d;
    }
  }

  Future<Map<String, dynamic>> rgb(Device d, Map<String, Object?> body) {
    if (!_id.hasMatch(d.id)) throw const FormatException('유효하지 않은 기기');
    return _request('account/m/${d.route}/kasadesk/rgb', body);
  }

  void close() {
    _closed = true;
    expired = null;
    server.close();
    _http.close();
  }
}
