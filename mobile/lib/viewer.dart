import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:kasadesk_protocol/screen_session.dart';
import 'connection.dart';
import 'android_video.dart';

Iterable<String> remoteTextChunks(String value) sync* {
  var chunk = StringBuffer(), bytes = 0;
  for (final rune in value.runes) {
    final ch = String.fromCharCode(rune);
    final size = utf8.encode(ch).length;
    if (bytes + size > 4000) {
      yield chunk.toString();
      chunk = StringBuffer();
      bytes = 0;
    }
    chunk.write(ch);
    bytes += size;
  }
  if (bytes > 0) yield chunk.toString();
}

enum InputMode { view, touch, trackpad }

class Viewer extends StatefulWidget {
  const Viewer({super.key, required this.link, required this.device});
  final DeskConnection link;
  final Device device;
  @override
  State<Viewer> createState() => _ViewerState();
}

class _ViewerState extends State<Viewer> with WidgetsBindingObserver {
  late final ScreenSession session;
  final zoom = TransformationController(), focus = FocusNode();
  InputMode mode = InputMode.view;
  bool dragging = false,
      highQuality = false,
      feedback = true,
      foreground = true;
  int held = 0, mods = 0;
  Offset cursor = const Offset(.5, .5);
  final fingers = <int, Offset>{};
  Offset? start;
  bool moved = false, multi = false;
  DateTime started = DateTime.now();
  String notice = '';
  Timer? hide, tick;
  bool get canInput =>
      foreground &&
      session.state == ScreenState.live &&
      session.hasPicture &&
      session.inputDenied == null &&
      mode != InputMode.view;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    session = ScreenSession(
      widget.link.server,
      machine: widget.device.route,
      video: Platform.isAndroid ? AndroidVideo() : null,
      wantVideo: (_) => highQuality,
    );
    session.addListener(change);
    session.connect();
    tick = Timer.periodic(const Duration(seconds: 1), (_) {
      if (mounted && foreground) setState(() {});
    });
  }

  void change() {
    if (session.state != ScreenState.live || session.inputDenied != null) {
      reset();
      mods = 0;
    }
    if (mounted) setState(() {});
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    foreground = state == AppLifecycleState.resumed;
    if (!foreground) {
      reset();
      mods = 0;
      session.pause();
    } else {
      session.resume();
    }
  }

  void reset() {
    if (held != 0) session.pointer(cursor.dx, cursor.dy, 0, modifiers: mods);
    held = 0;
    fingers.clear();
    start = null;
    multi = false;
    moved = false;
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    hide?.cancel();
    tick?.cancel();
    session.removeListener(change);
    reset();
    session.dispose();
    zoom.dispose();
    focus.dispose();
    super.dispose();
  }

  void showInput(String text) {
    if (!feedback) return;
    hide?.cancel();
    setState(() => notice = text);
    hide = Timer(const Duration(milliseconds: 1150), () {
      if (mounted) setState(() => notice = '');
    });
  }

  void setMode(InputMode m) {
    reset();
    setState(() {
      mode = m;
      mods = 0;
      dragging = false;
      zoom.value = Matrix4.identity();
      notice = '';
    });
    focus.requestFocus();
  }

  void key(String code) {
    if (!canInput) return;
    session.key(code, modifiers: mods);
    showInput(
      [
        if (mods & 2 != 0) 'Ctrl',
        if (mods & 4 != 0) 'Alt',
        if (mods & 1 != 0) 'Shift',
        if (mods & 8 != 0) 'Meta',
        code,
      ].join(' + '),
    );
    setState(() => mods = 0);
  }

  Future<void> keyboard() async {
    if (!canInput) return;
    final value = await showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      builder: (_) => const _Composer(),
    );
    if (!mounted || !canInput || value == null || value.isEmpty) return;
    if (remoteTextChunks(value).every(session.typeText)) {
      showInput('텍스트 전달 · ${value.characters.length}자');
    }
  }

  Rect rect(Size size) {
    final picture = session.pictureSize ?? const Size(16, 9);
    return Alignment.center.inscribe(
      applyBoxFit(BoxFit.contain, picture, size).destination,
      Offset.zero & size,
    );
  }

  Offset norm(Offset p, Rect r) => Offset(
    ((p.dx - r.left) / r.width).clamp(0, 1),
    ((p.dy - r.top) / r.height).clamp(0, 1),
  );
  void down(PointerDownEvent e, Rect r) {
    if (!canInput ||
        (mode == InputMode.touch && !r.contains(e.localPosition))) {
      return;
    }
    fingers[e.pointer] = e.localPosition;
    if (fingers.length > 1) {
      if (held != 0) {
        session.pointer(cursor.dx, cursor.dy, 0);
        held = 0;
      }
      multi = true;
      return;
    }
    multi = false;
    moved = false;
    start = e.localPosition;
    started = DateTime.now();
    if (mode == InputMode.touch || e.kind == PointerDeviceKind.mouse) {
      cursor = norm(e.localPosition, r);
    }
    if (dragging || e.kind == PointerDeviceKind.mouse) {
      held = e.buttons & 7;
      if (held == 0) held = 1;
      session.pointer(cursor.dx, cursor.dy, held, modifiers: mods);
      showInput(held == 2 ? '우클릭' : '누름');
    }
    setState(() {});
  }

  void move(PointerMoveEvent e, Rect r) {
    if (!canInput || !fingers.containsKey(e.pointer)) return;
    final previous = fingers[e.pointer]!;
    fingers[e.pointer] = e.localPosition;
    if (multi) {
      if (fingers.length == 2) {
        moved |= (e.localPosition - previous).distance > 1;
        final d = e.localPosition - previous;
        session.scroll(
          cursor.dx,
          cursor.dy,
          (d.dx * session.displaySize.width / r.width / 2).round(),
          (d.dy * session.displaySize.height / r.height / 2).round(),
        );
        showInput('스크롤');
      }
      return;
    }
    moved |= (e.localPosition - start!).distance > 6;
    cursor = mode == InputMode.touch || e.kind == PointerDeviceKind.mouse
        ? norm(e.localPosition, r)
        : Offset(
            (cursor.dx + e.delta.dx / r.width).clamp(0, 1),
            (cursor.dy + e.delta.dy / r.height).clamp(0, 1),
          );
    if (moved || held != 0) {
      session.pointer(cursor.dx, cursor.dy, held, modifiers: mods);
    }
    setState(() {});
  }

  void up(PointerUpEvent e) {
    if (fingers.remove(e.pointer) == null) return;
    if (held != 0) {
      session.pointer(cursor.dx, cursor.dy, 0, modifiers: mods);
      held = 0;
      showInput('놓기');
    } else if (!multi && !moved && canInput) {
      final right = DateTime.now().difference(started).inMilliseconds >= 450;
      session.click(cursor.dx, cursor.dy, right: right, modifiers: mods);
      showInput(right ? '우클릭' : '클릭');
    }
    if (fingers.isEmpty &&
        multi &&
        !moved &&
        canInput &&
        DateTime.now().difference(started).inMilliseconds < 450) {
      session.click(cursor.dx, cursor.dy, right: true, modifiers: mods);
      showInput('우클릭');
    }
    if (fingers.isEmpty) {
      start = null;
      multi = false;
      mods = 0;
    }
  }

  KeyEventResult hardware(FocusNode node, KeyEvent e) {
    if (!canInput ||
        ModalRoute.of(context)?.isCurrent != true ||
        e is KeyUpEvent) {
      return KeyEventResult.ignored;
    }
    final hk = HardwareKeyboard.instance;
    final m =
        mods |
        (hk.isControlPressed ? 2 : 0) |
        (hk.isAltPressed ? 4 : 0) |
        (hk.isMetaPressed ? 8 : 0) |
        (hk.isShiftPressed ? 1 : 0);
    final code = e.physicalKey.debugName?.replaceAll(' ', '');
    const special = {
      'Enter',
      'Tab',
      'Backspace',
      'Escape',
      'ArrowLeft',
      'ArrowRight',
      'ArrowDown',
      'ArrowUp',
      'Delete',
      'Home',
      'End',
    };
    if (e.character != null &&
        e.character!.runes.every((r) => r >= 32) &&
        (m & 14) == 0) {
      session.typeText(e.character!);
      showInput('문자 입력');
    } else if (code != null &&
        (special.contains(code) ||
            RegExp(r'^(Key[A-Z]|Digit[0-9]|F[0-9]{1,2})$').hasMatch(code))) {
      session.key(code, modifiers: m);
      showInput(
        [
          if (m & 2 != 0) 'Ctrl',
          if (m & 4 != 0) 'Alt',
          if (m & 8 != 0) 'Meta',
          if (m & 1 != 0) 'Shift',
          code.replaceFirst('Key', ''),
        ].join(' + '),
      );
    } else {
      return KeyEventResult.ignored;
    }
    mods = 0;
    return KeyEventResult.handled;
  }

  String get status =>
      session.note ??
      switch (session.state) {
        ScreenState.connecting => '화면 연결 중…',
        ScreenState.reconnecting => '다시 연결 중…',
        ScreenState.denied =>
          session.denied == 'permission'
              ? '컴퓨터에서 화면 기록 권한을 허용해 주세요.'
              : '화면을 사용할 수 없어요.',
        ScreenState.live =>
          session.inputDenied != null
              ? '컴퓨터에서 원격 입력 권한을 확인하세요.'
              : '${session.videoMode ? "영상" : "JPEG"} · ${session.fps} fps',
      };
  Widget surface(BuildContext c, BoxConstraints constraints) {
    final size = constraints.biggest, r = rect(size);
    final img = session.image;
    if (!session.hasPicture || session.state != ScreenState.live) {
      return Center(
        child: Text(status, style: const TextStyle(color: Colors.white70)),
      );
    }
    final picture = session.videoMode && session.textureId != null
        ? Texture(textureId: session.textureId!)
        : RawImage(image: img, fit: BoxFit.fill);
    final image = Stack(
      children: [
        Positioned.fromRect(rect: r, child: picture),
        if (mode == InputMode.trackpad)
          Positioned(
            left: (r.left + cursor.dx * r.width - 7).clamp(0, size.width - 14),
            top: (r.top + cursor.dy * r.height - 7).clamp(0, size.height - 14),
            child: const IgnorePointer(
              child: Icon(Icons.adjust, size: 15, color: Colors.amberAccent),
            ),
          ),
      ],
    );
    return Stack(
      fit: StackFit.expand,
      children: [
        if (mode == InputMode.view)
          InteractiveViewer(
            transformationController: zoom,
            minScale: 1,
            maxScale: 5,
            child: image,
          )
        else
          Listener(
            behavior: HitTestBehavior.opaque,
            onPointerDown: (e) => down(e, r),
            onPointerMove: (e) => move(e, r),
            onPointerUp: up,
            onPointerCancel: (_) => reset(),
            onPointerHover: (e) {
              if (canInput &&
                  e.kind == PointerDeviceKind.mouse &&
                  r.contains(e.localPosition)) {
                cursor = norm(e.localPosition, r);
                session.pointer(cursor.dx, cursor.dy, 0, modifiers: mods);
                setState(() {});
              }
            },
            onPointerSignal: (e) {
              if (canInput && e is PointerScrollEvent) {
                session.scroll(
                  cursor.dx,
                  cursor.dy,
                  -e.scrollDelta.dx.round(),
                  -e.scrollDelta.dy.round(),
                );
                showInput('스크롤');
              }
            },
            child: image,
          ),
        if (notice.isNotEmpty && feedback)
          Positioned(
            left: 12,
            bottom: 10,
            child: IgnorePointer(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: const Color(0xe6233532),
                  borderRadius: BorderRadius.circular(12),
                ),
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 7,
                  ),
                  child: Text(
                    notice,
                    style: const TextStyle(color: Color(0xffa6e0c9)),
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }

  @override
  Widget build(BuildContext c) => Scaffold(
    backgroundColor: const Color(0xff182220),
    appBar: AppBar(
      title: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            widget.device.name,
            style: const TextStyle(fontSize: 17, fontWeight: FontWeight.bold),
          ),
          Text(status, style: const TextStyle(fontSize: 12)),
        ],
      ),
      actions: [
        if (session.displays.length > 1)
          PopupMenuButton<int>(
            tooltip: '모니터 선택',
            icon: const Icon(Icons.monitor),
            onSelected: (id) {
              reset();
              zoom.value = Matrix4.identity();
              session.pickDisplay(id);
            },
            itemBuilder: (_) => [
              for (final d in session.displays)
                PopupMenuItem(
                  value: d.id,
                  child: Text('모니터 ${d.id} · ${d.width}×${d.height}'),
                ),
            ],
          ),
        PopupMenuButton<String>(
          onSelected: (v) {
            if (v == 'retry') {
              reset();
              session.inputDenied = null;
              session.connect();
              return;
            }
            if (v == 'quality') {
              reset();
              highQuality = !highQuality;
              session.connect();
            } else {
              setState(() {
                feedback = !feedback;
                notice = '';
              });
            }
          },
          itemBuilder: (_) => [
            const PopupMenuItem(value: 'retry', child: Text('다시 연결')),
            CheckedPopupMenuItem(
              value: 'feedback',
              checked: feedback,
              child: const Text('입력 표시'),
            ),
            if (session.textureId != null)
              CheckedPopupMenuItem(
                value: 'quality',
                checked: highQuality,
                child: const Text('고화질 영상 요청'),
              ),
          ],
        ),
      ],
    ),
    body: SafeArea(
      child: Focus(
        focusNode: focus,
        autofocus: true,
        onKeyEvent: hardware,
        child: Column(
          children: [
            Expanded(child: LayoutBuilder(builder: surface)),
            Material(
              color: const Color(0xfff5f4ee),
              child: Padding(
                padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
                child: Column(
                  children: [
                    SegmentedButton<InputMode>(
                      showSelectedIcon: false,
                      segments: const [
                        ButtonSegment(
                          value: InputMode.view,
                          label: Text('보기'),
                          icon: Icon(Icons.visibility_outlined),
                        ),
                        ButtonSegment(
                          value: InputMode.touch,
                          label: Text('터치'),
                          icon: Icon(Icons.touch_app_outlined),
                        ),
                        ButtonSegment(
                          value: InputMode.trackpad,
                          label: Text('트랙패드'),
                          icon: Icon(Icons.mouse_outlined),
                        ),
                      ],
                      selected: {mode},
                      onSelectionChanged: (v) => setMode(v.first),
                    ),
                    if (mode != InputMode.view)
                      SingleChildScrollView(
                        scrollDirection: Axis.horizontal,
                        child: Row(
                          children: [
                            IconButton(
                              tooltip: '키보드',
                              onPressed: canInput ? keyboard : null,
                              icon: const Icon(Icons.keyboard_alt_outlined),
                            ),
                            FilterChip(
                              label: const Text('드래그'),
                              selected: dragging,
                              onSelected: (v) {
                                reset();
                                setState(() => dragging = v);
                              },
                            ),
                            for (final (name, bit) in [
                              ('Ctrl', 2),
                              ('Alt', 4),
                              ('Shift', 1),
                              ('Meta', 8),
                            ])
                              Padding(
                                padding: const EdgeInsets.only(left: 4),
                                child: FilterChip(
                                  label: Text(name),
                                  selected: (mods & bit) != 0,
                                  onSelected: canInput
                                      ? (_) => setState(() => mods ^= bit)
                                      : null,
                                ),
                              ),
                            for (final (label, code) in [
                              ('Esc', 'Escape'),
                              ('Tab', 'Tab'),
                              ('Enter', 'Enter'),
                              ('⌫', 'Backspace'),
                              ('←', 'ArrowLeft'),
                              ('↑', 'ArrowUp'),
                              ('↓', 'ArrowDown'),
                              ('→', 'ArrowRight'),
                              ('A', 'KeyA'),
                              ('C', 'KeyC'),
                              ('V', 'KeyV'),
                              ('Z', 'KeyZ'),
                            ])
                              TextButton(
                                onPressed: canInput
                                    ? () {
                                        key(code);
                                      }
                                    : null,
                                child: Text(label),
                              ),
                          ],
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    ),
  );
}

class _Composer extends StatefulWidget {
  const _Composer();
  @override
  State<_Composer> createState() => _ComposerState();
}

class _ComposerState extends State<_Composer> {
  final controller = TextEditingController();
  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext c) => Padding(
    padding: EdgeInsets.fromLTRB(
      16,
      16,
      16,
      MediaQuery.viewInsetsOf(c).bottom + 12,
    ),
    child: Row(
      children: [
        Expanded(
          child: TextField(
            autofocus: true,
            controller: controller,
            decoration: const InputDecoration(labelText: '원격 기기에 입력할 글'),
            minLines: 1,
            maxLines: 3,
          ),
        ),
        IconButton(
          tooltip: '전송',
          onPressed: () => Navigator.pop(c, controller.text),
          icon: const Icon(Icons.send),
        ),
      ],
    ),
  );
}
