import 'dart:async';
import 'previews.dart';
import 'dart:ui' as ui;
import 'dart:math' as math;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:vector_math/vector_math_64.dart' show Vector3;
import 'connection.dart';
import 'lighting.dart';

typedef RoomTarget = ({Device device, bool lights});

class Room extends StatefulWidget {
  const Room({
    super.key,
    required this.devices,
    required this.onPick,
    required this.link,
    required this.active,
    required this.colors,
  });
  final DeskConnection link;
  final bool active;
  final List<Device> devices;
  final ValueChanged<RoomTarget> onPick;
  final Map<String, RoomLights> colors;
  @override
  State<Room> createState() => _RoomState();
}

class _RoomState extends State<Room> with WidgetsBindingObserver {
  final loader = RoomPreview();
  final images = <String, ui.Image>{};
  Timer? timer;
  int next = 0;
  bool reading = false;
  bool get visible =>
      widget.active &&
      WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed;
  @override
  void didUpdateWidget(Room old) {
    super.didUpdateWidget(old);
    if (!visible) loader.cancel();
    final ids = widget.devices.map((d) => d.id).toSet();
    for (final id in images.keys.where((id) => !ids.contains(id)).toList()) {
      images.remove(id)?.dispose();
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (!visible) loader.cancel();
  }

  Future<void> preview() async {
    if (!visible || reading) return;
    final visibleDevices = widget.devices.skip(page * 4).take(4).toList();
    if (visibleDevices.isEmpty) return;
    final device = visibleDevices[next++ % visibleDevices.length];
    reading = true;
    try {
      final image = await loader.capture(widget.link, device);
      if (!mounted ||
          !visible ||
          !widget.devices.any((d) => d.id == device.id)) {
        image?.dispose();
        return;
      }
      final old = images.remove(device.id);
      if (image != null) images[device.id] = image;
      setState(() {});
      WidgetsBinding.instance.addPostFrameCallback((_) {
        old?.dispose();
      });
    } finally {
      reading = false;
    }
  }

  double yaw = .16, pitch = .33, zoom = 1, startZoom = 1;
  int page = 0;
  ui.Image? river;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    timer = Timer.periodic(const Duration(seconds: 3), (_) => preview());
    load();
  }

  Future<void> load() async {
    final bytes = await rootBundle.load('assets/river.jpg');
    final codec = await ui.instantiateImageCodec(
      bytes.buffer.asUint8List(bytes.offsetInBytes, bytes.lengthInBytes),
      targetWidth: 1024,
    );
    final frame = await codec.getNextFrame();
    codec.dispose();
    if (!mounted) {
      frame.image.dispose();
      return;
    }
    setState(() => river = frame.image);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    timer?.cancel();
    loader.cancel();
    for (final i in images.values) {
      i.dispose();
    }
    river?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext c) {
    final pages = math.max(1, (widget.devices.length + 3) ~/ 4);
    page = page.clamp(0, pages - 1);
    final painter = _Scene(
      widget.devices.skip(page * 4).take(4).toList(),
      widget.colors,
      images,
      yaw,
      pitch,
      zoom,
      river,
    );
    return Stack(
      fit: StackFit.expand,
      children: [
        GestureDetector(
          onScaleStart: (_) => startZoom = zoom,
          onScaleUpdate: (d) => setState(() {
            yaw = (yaw - d.focalPointDelta.dx * .008).clamp(-1.0, 1.0);
            pitch = (pitch + d.focalPointDelta.dy * .006).clamp(.12, .80);
            zoom = (startZoom * d.scale).clamp(.72, 1.8);
          }),
          onTapUp: (d) {
            for (final face in painter.faces.reversed) {
              if (face.path.contains(d.localPosition)) {
                if (face.target != null) widget.onPick(face.target!);
                break;
              }
            }
          },
          child: CustomPaint(painter: painter),
        ),
        Positioned(
          top: 8,
          right: 8,
          child: IconButton.filledTonal(
            tooltip: '시점 복귀',
            icon: const Icon(Icons.center_focus_strong, size: 18),
            onPressed: () => setState(() {
              yaw = .16;
              pitch = .33;
              zoom = 1;
            }),
          ),
        ),
        if (pages > 1)
          Positioned(
            bottom: 6,
            right: 6,
            child: Material(
              color: Colors.white70,
              borderRadius: BorderRadius.circular(20),
              child: Row(
                children: [
                  IconButton(
                    tooltip: '이전 기기',
                    onPressed: page > 0 ? () => setState(() => page--) : null,
                    icon: const Icon(Icons.chevron_left),
                  ),
                  Text('${page + 1}/$pages'),
                  IconButton(
                    tooltip: '다음 기기',
                    onPressed: page + 1 < pages
                        ? () => setState(() => page++)
                        : null,
                    icon: const Icon(Icons.chevron_right),
                  ),
                ],
              ),
            ),
          ),
      ],
    );
  }
}

class _Face {
  _Face(
    this.path,
    this.z,
    this.color,
    this.target, {
    this.window,
    this.vertices,
  });
  final Path path;
  final double z;
  final Color color;
  final RoomTarget? target;
  final ui.Image? window;
  final List<Offset>? vertices;
}

class _Scene extends CustomPainter {
  _Scene(
    this.devices,
    this.colors,
    this.images,
    this.yaw,
    this.pitch,
    this.zoom,
    this.river,
  );
  final Map<String, ui.Image> images;
  final List<Device> devices;
  final Map<String, RoomLights> colors;
  final double yaw, pitch, zoom;
  final ui.Image? river;
  final faces = <_Face>[];
  @override
  void paint(Canvas canvas, Size size) {
    faces.clear();
    final batch = <_Face>[];
    final n = devices.length, span = math.max(3.5, 2.65 * n);
    final dist = math.max(7.3, span * .93),
        focal =
            math.min(size.width * dist / (span + 2.1), size.height * 1.26) *
            zoom;
    final eye = Vector3(
      math.sin(yaw) * math.cos(pitch),
      math.sin(pitch),
      math.cos(yaw) * math.cos(pitch),
    );
    final right = Vector3(math.cos(yaw), 0, -math.sin(yaw)),
        up = eye.cross(right);
    final target = Vector3(0, 1.4, 0);
    ({Offset at, double z}) project(Vector3 v) {
      final d = v - target;
      final z = dist - d.dot(eye);
      return (
        at: Offset(
          size.width * .5 + d.dot(right) * focal / z,
          size.height * .58 - d.dot(up) * focal / z,
        ),
        z: z,
      );
    }

    void face(
      List<Vector3> pts,
      Color col, {
      RoomTarget? hit,
      ui.Image? window,
    }) {
      final p = pts.map(project).toList();
      if (p.any((e) => e.z <= .05)) return;
      final xy = p.map((e) => e.at).toList();
      batch.add(
        _Face(
          Path()..addPolygon(xy, true),
          p.map((e) => e.z).reduce((a, b) => a + b) / p.length,
          col,
          hit,
          window: window,
          vertices: window == null ? null : xy,
        ),
      );
    }

    void box(
      double x,
      double y,
      double z,
      double w,
      double h,
      double d,
      Color c, {
      RoomTarget? hit,
    }) {
      final pts = [
        for (final yy in [y, y + h])
          for (final zz in [z - d / 2, z + d / 2])
            for (final xx in [x - w / 2, x + w / 2]) Vector3(xx, yy, zz),
      ];
      final sides = [
        ([0, 1, 3, 2], .65),
        ([0, 4, 5, 1], .80),
        ([0, 2, 6, 4], .71),
        ([1, 5, 7, 3], .84),
        ([2, 3, 7, 6], .94),
        ([4, 6, 7, 5], 1.0),
      ];
      for (final (ix, k) in sides) {
        face(
          ix.map((i) => pts[i]).toList(),
          Color.lerp(Colors.black, c, k)!,
          hit: hit,
        );
      }
    }

    const wood = Color(0xffd9ad80),
        white = Color(0xffeef0e9),
        metal = Color(0xff929f9d),
        dark = Color(0xff293638);
    canvas.drawRect(
      Offset.zero & size,
      Paint()
        ..shader = const LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [Color(0xff8eb9cf), Color(0xffe9dcbb)],
        ).createShader(Offset.zero & size),
    );
    final half = span / 2 + 1.9;
    // Room surfaces are behind the furniture regardless of their large face centroids.
    final wall = [
      Vector3(-half, 5, -2.15),
      Vector3(half, 5, -2.15),
      Vector3(half, .3, -2.15),
      Vector3(-half, .3, -2.15),
    ];
    face(wall, const Color(0xffb1c7ce), window: river);
    box(0, .04, -2.2, half * 2, .3, .1, white);
    for (final x in [-half, -half / 2, 0.0, half / 2, half]) {
      box(x, .25, -2.13, .07, 4.8, .08, dark);
    }
    void drawFaces() {
      batch.sort((a, b) => b.z.compareTo(a.z));
      faces.addAll(batch);
      for (final f in batch) {
        if (f.window case final img?) {
          final shader = ui.ImageShader(
            img,
            TileMode.clamp,
            TileMode.clamp,
            Matrix4.identity().storage,
          );
          final w = img.width.toDouble(), h = img.height.toDouble();
          final mesh = ui.Vertices(
            ui.VertexMode.triangleFan,
            f.vertices!,
            textureCoordinates: [
              Offset.zero,
              Offset(w, 0),
              Offset(w, h),
              Offset(0, h),
            ],
          );
          canvas.drawVertices(
            mesh,
            BlendMode.srcOver,
            Paint()
              ..shader = shader
              ..filterQuality = FilterQuality.low,
          );
          mesh.dispose();
          shader.dispose();
        } else {
          canvas.drawPath(f.path, Paint()..color = f.color);
        }
      }
      batch.clear();
    }

    drawFaces();
    void plane(
      double x,
      double y,
      double z,
      double w,
      double d,
      Color col, {
      RoomTarget? hit,
    }) {
      face(
        [
          Vector3(x - w / 2, y, z - d / 2),
          Vector3(x + w / 2, y, z - d / 2),
          Vector3(x + w / 2, y, z + d / 2),
          Vector3(x - w / 2, y, z + d / 2),
        ],
        col,
        hit: hit,
      );
    }

    plane(0, -.68, .8, half * 2, 6.0, const Color(0xffd4c3ab));
    drawFaces();
    for (var i = -10; i <= 10; i++) {
      plane(i * .55, -.675, .8, .012, 6.0, const Color(0xffbfae97));
    }
    plane(.16, -.67, .24, span + 1.35, 2.94, const Color(0xff87928a));
    drawFaces();
    plane(.14, -.66, .22, span + 1.23, 2.82, const Color(0xffa2ada6));
    drawFaces();

    box(0, 1, 0, span + 1, .12, 2.1, wood);
    for (final x in [-span / 2, span / 2]) {
      for (final z in [-.77, .77]) {
        box(x, -.65, z, .13, 1.65, .13, dark);
      }
    }
    drawFaces();
    const table = 1.12;
    plane(0, table + .001, 0, span + .52, 1.76, const Color(0xff586866));
    drawFaces();
    final order = List.generate(n, (i) => i)
      ..sort((a, b) => yaw >= 0 ? a.compareTo(b) : b.compareTo(a));
    for (final i in order) {
      final d = devices[i], x = (i - (n - 1) / 2) * 2.65;
      final screen = (device: d, lights: false),
          lights = (device: d, lights: true);
      final laptop = d.shape == Shape.notebook;
      final hasCase = d.lights || d.shape == Shape.desktop;
      final sx = x - (hasCase ? 0.42 : 0.0);
      plane(sx + .07, table + .003, .08, 1.77, 1.12, const Color(0xff455451));
      drawFaces();
      if (laptop) {
        box(sx, table, .05, 1.60, .08, 1.0, metal, hit: screen);
        drawFaces();
        plane(sx, table + .082, -.13, 1.31, .38, dark, hit: screen);
        drawFaces();
        for (var row = 0; row < 4; row++) {
          for (var k = 0; k < 12; k++) {
            plane(
              sx + (k - 5.5) * .104,
              table + .09,
              -.26 + row * .085,
              .088,
              .065,
              metal,
              hit: screen,
            );
          }
        }
        plane(
          sx,
          table + .091,
          .28,
          .44,
          .25,
          const Color(0xffb9c3c2),
          hit: screen,
        );
        drawFaces();
      } else {
        box(sx, table, -.1, .68, .06, .46, metal, hit: screen);
        box(sx, table + .06, -.17, .13, .39, .11, metal, hit: screen);
      }
      final sy = table + (laptop ? 0.1 : 0.35),
          sw = laptop ? 1.6 : 1.85,
          sh = laptop ? 0.99 : 1.06;
      box(sx, sy, -.4, sw, sh, .09, dark, hit: screen);
      box(
        sx,
        sy + .045,
        -.345,
        sw - .095,
        sh - .09,
        .015,
        const Color(0xff507780),
        hit: screen,
      );
      drawFaces();
      final shot = images[d.id];
      if (shot != null) {
        final ratio = shot.width / shot.height;
        final iw = math.min(sw - .095, (sh - .09) * ratio), ih = iw / ratio;
        final cy = sy + sh / 2;
        face(
          [
            Vector3(sx - iw / 2, cy + ih / 2, -.335),
            Vector3(sx + iw / 2, cy + ih / 2, -.335),
            Vector3(sx + iw / 2, cy - ih / 2, -.335),
            Vector3(sx - iw / 2, cy - ih / 2, -.335),
          ],
          dark,
          hit: screen,
          window: shot,
        );
      }
      drawFaces();
      if (!laptop) {
        box(sx, table + .009, .54, .98, .027, .34, metal, hit: screen);
        drawFaces();
        for (var row = 0; row < 3; row++) {
          for (var k = 0; k < 11; k++) {
            plane(
              sx + (k - 5) * .083,
              table + .038,
              .44 + row * .091,
              .07,
              .069,
              white,
              hit: screen,
            );
          }
        }
        box(sx + .73, table + .009, .51, .14, .054, .24, white, hit: screen);
        drawFaces();
      }
      if (d.shape == Shape.mini) {
        box(sx + .57, table, -.11, .55, .17, .45, white, hit: screen);
      }
      if (hasCase) {
        final tx = x + .94;
        final caseHit = d.lights ? lights : screen;
        box(tx, table, -.03, .75, 1.25, .94, white, hit: caseHit);
        box(
          tx,
          table + .10,
          .451,
          .64,
          1.02,
          .025,
          const Color(0xff606e72),
          hit: caseHit,
        );
        drawFaces();
        final light = colors[d.id];
        for (final yy in [1.56, 1.93]) {
          final glow =
              light?.fans[yy < 1.7 ? 'right2' : 'right1'] ??
              const Color(0xffadc9bd);
          final c = Vector3(tx + .16, yy, .48);
          for (final (radius, shade) in [
            (.154, glow),
            (.104, dark),
            (.04, glow),
          ]) {
            face(
              [
                for (var k = 0; k < 20; k++)
                  Vector3(
                    c.x + radius * math.cos(k * math.pi / 10),
                    c.y + radius * math.sin(k * math.pi / 10),
                    c.z + (0.155 - radius) * .025,
                  ),
              ],
              shade,
              hit: caseHit,
            );
          }
        }
        drawFaces();
        box(tx - .15, table + .79, .48, .18, .18, .035, dark, hit: caseHit);
        box(tx - .12, table + .24, .49, .30, .28, .026, white, hit: caseHit);
        box(
          tx - .12,
          table + .39,
          .514,
          .26,
          .032,
          .01,
          light?.gpu ?? const Color(0xff8bbfd0),
          hit: caseHit,
        );
      }
      drawFaces();
    }
  }

  @override
  bool shouldRepaint(_Scene old) => true;
}
