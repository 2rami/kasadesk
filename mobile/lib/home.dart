import 'dart:async';
import 'package:flutter/material.dart';
import 'connection.dart';
import 'room.dart';
import 'viewer.dart';
import 'lighting.dart';

IconData deviceIcon(Shape s) => switch (s) {
  Shape.notebook => Icons.laptop_mac,
  Shape.mini => Icons.dns_outlined,
  Shape.desktop => Icons.desktop_windows_outlined,
  _ => Icons.monitor,
};

class DeskHome extends StatefulWidget {
  const DeskHome({super.key, required this.link, required this.logout});
  final DeskConnection link;
  final Future<void> Function() logout;
  @override
  State<DeskHome> createState() => _DeskHomeState();
}

class _DeskHomeState extends State<DeskHome> with WidgetsBindingObserver {
  List<Device> devices = [];
  Map<String, RoomLights> colors = {};
  bool loading = false;
  bool roomActive = true;
  String? error;
  Timer? timer;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    refresh();
    timer = Timer.periodic(const Duration(seconds: 30), (_) {
      if (ModalRoute.of(context)?.isCurrent == true &&
          WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed) {
        refresh();
      }
    });
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) refresh();
  }

  @override
  void dispose() {
    timer?.cancel();
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  Future<void> refresh() async {
    if (loading) return;
    setState(() => loading = true);
    try {
      final list = await widget.link.devices();
      if (!mounted) return;
      setState(() {
        devices = list;
        error = null;
        colors.removeWhere((k, v) => !list.any((d) => d.id == k && d.lights));
      });
      for (final d in list.where((e) => e.lights)) {
        try {
          final raw = await widget.link.rgb(d, {'op': 'snapshot'});
          if (mounted && ModalRoute.of(context)?.isCurrent == true) {
            setState(() => colors[d.id] = RoomLights.from(raw));
          }
        } catch (_) {}
      }
    } catch (_) {
      if (mounted) setState(() => error = '기기 목록을 새로 고칠 수 없어요.');
    } finally {
      if (mounted) setState(() => loading = false);
    }
  }

  Future<void> pick(RoomTarget t) async {
    setState(() => roomActive = false);
    RoomLights? confirmed = colors[t.device.id];
    if (t.lights) {
      await showModalBottomSheet<void>(
        context: context,
        useSafeArea: true,
        isScrollControlled: true,
        showDragHandle: true,
        builder: (_) => Lighting(
          link: widget.link,
          device: t.device,
          onConfirmed: (v) => confirmed = v,
          onLights: (v) {
            if (mounted) setState(() => colors[t.device.id] = v);
          },
        ),
      );
    } else {
      await Navigator.of(context).push(
        MaterialPageRoute<void>(
          builder: (_) => Viewer(link: widget.link, device: t.device),
        ),
      );
    }
    if (mounted) {
      setState(() {
        roomActive = true;
        if (t.lights) {
          if (confirmed == null) {
            colors.remove(t.device.id);
          } else {
            colors[t.device.id] = confirmed!;
          }
        }
      });
      unawaited(refresh());
    }
  }

  void profile() => showModalBottomSheet<void>(
    context: context,
    showDragHandle: true,
    builder: (c) => SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(24, 0, 24, 24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text('내 계정', style: Theme.of(c).textTheme.labelLarge),
            Text(
              widget.link.session.label,
              style: Theme.of(c).textTheme.headlineSmall,
            ),
            const Divider(height: 32),
            ListTile(
              contentPadding: EdgeInsets.zero,
              title: const Text('계정 ID'),
              subtitle: SelectableText(widget.link.session.account),
            ),
            TextButton(
              onPressed: () {
                Navigator.pop(c);
                widget.logout();
              },
              child: const Text('이 기기에서 로그아웃'),
            ),
          ],
        ),
      ),
    ),
  );
  @override
  Widget build(BuildContext c) {
    final room = ClipRRect(
      borderRadius: BorderRadius.circular(20),
      child: Room(
        devices: devices,
        colors: colors,
        onPick: pick,
        link: widget.link,
        active: roomActive,
      ),
    );
    final list = RefreshIndicator(
      onRefresh: refresh,
      child: ListView(
        padding: const EdgeInsets.all(16),
        physics: const AlwaysScrollableScrollPhysics(),
        children: [
          Row(
            children: [
              Expanded(
                child: Text('연결된 기기', style: Theme.of(c).textTheme.titleLarge),
              ),
              Text('${devices.length}대'),
              IconButton(
                tooltip: '새로 고침',
                onPressed: loading ? null : refresh,
                icon: const Icon(Icons.refresh),
              ),
            ],
          ),
          if (loading) const LinearProgressIndicator(minHeight: 2),
          if (error != null)
            Text(error!)
          else if (!loading && devices.isEmpty)
            const Padding(
              padding: EdgeInsets.symmetric(vertical: 22),
              child: Text('같은 카사계정으로 로그인한 컴퓨터에서 카사텀을 열어 주세요.'),
            ),
          for (final d in devices)
            Card(
              elevation: 0,
              color: Colors.white,
              child: ListTile(
                leading: Icon(deviceIcon(d.shape)),
                title: Text(d.name),
                subtitle: const Text('원격 화면'),
                trailing: d.lights
                    ? IconButton(
                        tooltip: '본체 조명',
                        onPressed: () => pick((device: d, lights: true)),
                        icon: const Icon(Icons.tune),
                      )
                    : const Icon(Icons.chevron_right),
                onTap: () => pick((device: d, lights: false)),
              ),
            ),
        ],
      ),
    );
    return Scaffold(
      appBar: AppBar(
        title: const Text(
          'KasaDesk',
          style: TextStyle(fontWeight: FontWeight.w700, letterSpacing: -1),
        ),
        actions: [
          IconButton(
            tooltip: '내 계정',
            onPressed: profile,
            icon: const Icon(Icons.account_circle_outlined),
          ),
        ],
      ),
      body: SafeArea(
        child: LayoutBuilder(
          builder: (c, box) =>
              box.maxWidth > 640 && box.maxWidth > box.maxHeight
              ? Row(
                  children: [
                    Expanded(
                      flex: 3,
                      child: Padding(
                        padding: const EdgeInsets.all(12),
                        child: room,
                      ),
                    ),
                    Expanded(flex: 2, child: list),
                  ],
                )
              : Column(
                  children: [
                    SizedBox(
                      height: box.maxHeight * .46,
                      child: Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 14),
                        child: room,
                      ),
                    ),
                    Expanded(child: list),
                  ],
                ),
        ),
      ),
    );
  }
}
