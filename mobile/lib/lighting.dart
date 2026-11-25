import 'dart:async';
import 'dart:convert';
import 'package:flutter/material.dart';
import 'connection.dart';

const spots = ['top1', 'top2', 'top3', 'left1', 'right1', 'right2', 'gpu'];
const names = ['상단 1', '상단 2', '상단 3', '후면', '측면 상단', '측면 하단', 'NEPTUNE GPU'];
Color? parseHex(Object? s) {
  if (s is! String || !RegExp(r'^[\da-fA-F]{6}$').hasMatch(s)) return null;
  return Color(0xff000000 | int.parse(s, radix: 16));
}

String hex(Color c) =>
    (c.toARGB32() & 0xffffff).toRadixString(16).padLeft(6, '0').toUpperCase();

class RoomLights {
  const RoomLights(this.fans, this.gpu);
  final Map<String, Color?> fans;
  final Color? gpu;
  factory RoomLights.from(Map m) {
    final g = m['gpu_light'];
    return RoomLights(
      {
        for (final v in (m['fans'] as List? ?? []).whereType<Map>())
          if (v['spot'] is String)
            v['spot']: m['connected'] == true
                ? parseHex(v['outer'] ?? v['fixed'])
                : null,
      },
      g is Map &&
              m['connected'] == true &&
              g['sync'] == true &&
              g['controller'] == true
          ? (g['asleep'] == true
                ? Colors.black
                : parseHex(g['fixed'] ?? g['sample']))
          : null,
    );
  }
}

class Lighting extends StatefulWidget {
  const Lighting({
    super.key,
    required this.link,
    required this.device,
    required this.onLights,
    required this.onConfirmed,
  });
  final DeskConnection link;
  final Device device;
  final ValueChanged<RoomLights> onLights;
  final ValueChanged<RoomLights?> onConfirmed;
  @override
  State<Lighting> createState() => _LightingState();
}

class _LightingState extends State<Lighting> with WidgetsBindingObserver {
  final field = TextEditingController();
  Map<String, dynamic>? state;
  int part = 0, gen = 0;
  bool saving = false, dirty = false, polling = false, thermal = false;
  String? error;
  Timer? timer;
  Map? reading(Map? s) {
    if (part == 6) return s?['gpu_light'] as Map?;
    return (s?['fans'] as List? ?? [])
        .whereType<Map>()
        .where((f) => f['spot'] == spots[part])
        .firstOrNull;
  }

  bool get available {
    final r = reading(state);
    return state?['connected'] == true &&
        (r?[part == 6 ? 'controller' : 'mapped'] == true);
  }

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    poll();
    timer = Timer.periodic(const Duration(seconds: 3), (_) {
      if (WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed) {
        poll();
      }
    });
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState s) {
    if (s == AppLifecycleState.resumed) poll();
  }

  @override
  void dispose() {
    gen++;
    WidgetsBinding.instance.removeObserver(this);
    timer?.cancel();
    field.dispose();
    super.dispose();
  }

  HSVColor basis = const HSVColor.fromAHSV(1, 0, 0, 1);

  void resetDraft() {
    final r = reading(state);
    field.text = (r?['fixed'] ?? r?['sample'] ?? r?['inner'] ?? '8FCBC1')
        .toString()
        .toUpperCase();
    basis = HSVColor.fromColor(parseHex(field.text) ?? Colors.white);
    dirty = false;
  }

  void updateRoom() {
    final color = parseHex(field.text);
    if (color != null && HSVColor.fromColor(color).value > 0) {
      basis = HSVColor.fromColor(color);
    }
    if (state == null) return;
    final real = RoomLights.from(state!), draft = parseHex(field.text);
    widget.onLights(
      !dirty || draft == null
          ? real
          : part == 6
          ? RoomLights(real.fans, draft)
          : RoomLights({...real.fans, spots[part]: draft}, real.gpu),
    );
  }

  Future<void> poll() async {
    if (saving || polling) return;
    polling = true;
    final id = gen;
    try {
      final s = await widget.link.rgb(widget.device, {'op': 'snapshot'});
      if (!mounted || id != gen) return;
      setState(() {
        state = s;
        error = null;
        if (!dirty) resetDraft();
      });
      widget.onConfirmed(RoomLights.from(s));
      updateRoom();
    } catch (_) {
      if (mounted && id == gen) {
        setState(() {
          state = null;
          error = '조명 상태를 읽지 못했어요.';
        });
      }
    } finally {
      polling = false;
    }
  }

  Future<void> write(String? color) async {
    if (!available || saving) return;
    gen++;
    setState(() => saving = true);
    try {
      final result = await widget.link.rgb(widget.device, {
        'op': 'color',
        'spot': spots[part],
        'color': color,
      });
      if (result['applied'] != true) throw const FormatException();
      final s = await widget.link.rgb(widget.device, {'op': 'snapshot'});
      if (!mounted) return;
      final confirmed =
          reading(s)?.containsKey('fixed') == true &&
          (reading(s)?['fixed'] as String?)?.toUpperCase() == color;
      setState(() {
        state = s;
        resetDraft();
        error = confirmed ? null : '저장값이 달라요. 현재 값을 확인해 주세요.';
      });
      widget.onConfirmed(RoomLights.from(s));
      updateRoom();
    } catch (_) {
      if (mounted) setState(() => error = '저장 결과를 확인할 수 없어요.');
    } finally {
      if (mounted) setState(() => saving = false);
    }
  }

  @override
  Widget build(BuildContext c) {
    final r = reading(state);
    final fixed = parseHex(r?['fixed']);
    final g = state?['gpu_light'];
    final lcd = state?['lcd'];
    return FractionallySizedBox(
      heightFactor: .82,
      child: ListView(
        padding: EdgeInsets.fromLTRB(
          20,
          0,
          20,
          MediaQuery.viewInsetsOf(c).bottom + 24,
        ),
        children: [
          Text(widget.device.name, style: Theme.of(c).textTheme.headlineSmall),
          const SizedBox(height: 10),
          SegmentedButton<bool>(
            segments: const [
              ButtonSegment(value: false, label: Text('조명')),
              ButtonSegment(value: true, label: Text('냉각 · LCD')),
            ],
            selected: {thermal},
            onSelectionChanged: (s) => setState(() => thermal = s.first),
          ),
          if (thermal) ...[
            if (lcd is String && lcd.isNotEmpty)
              Center(
                child: ClipOval(
                  child: Image.memory(
                    base64Decode(lcd),
                    width: 136,
                    height: 136,
                    errorBuilder: (_, _, _) => const Icon(Icons.broken_image),
                  ),
                ),
              ),
            for (final entry in [
              ('냉각수', 'liquid', '°C'),
              ('펌프', 'pump', ' RPM'),
              ('GPU', 'gpu', '°C'),
            ])
              ListTile(
                title: Text(entry.$1),
                trailing: Text(
                  state?[entry.$2] == null
                      ? '—'
                      : '${state![entry.$2]}${entry.$3}',
                ),
              ),
          ] else ...[
            const SizedBox(height: 14),
            Wrap(
              spacing: 6,
              children: [
                for (var i = 0; i < spots.length; i++)
                  ChoiceChip(
                    label: Text(names[i]),
                    selected: part == i,
                    onSelected: saving
                        ? null
                        : (_) {
                            setState(() {
                              part = i;
                              resetDraft();
                            });
                            updateRoom();
                          },
                  ),
              ],
            ),
            ListTile(
              contentPadding: EdgeInsets.zero,
              leading: CircleAvatar(
                backgroundColor:
                    fixed ??
                    parseHex(r?['sample'] ?? r?['outer']) ??
                    Colors.grey,
              ),
              title: Text(
                r == null
                    ? '상태 확인 대기'
                    : fixed == null
                    ? '효과 따르기'
                    : '고정 색  #${hex(fixed)}',
              ),
              subtitle: Text(
                part == 6
                    ? (g is! Map
                          ? 'GPU 조명 지원 호스트가 필요해요'
                          : g['asleep'] == true
                          ? '모니터 연동 소등'
                          : g['sync'] != true
                          ? '장치 동기화 꺼짐'
                          : 'Razer ARGB · 채널 5')
                    : (r?['rpm'] == null ? '팬 조명' : '${r!['rpm']} RPM'),
              ),
            ),
            Wrap(
              spacing: 6,
              children: [
                for (final c in [
                  0xfffff4df,
                  0xffef996f,
                  0xffe66c70,
                  0xffc288d9,
                  0xff838ef2,
                  0xff6eaefa,
                  0xff73dbc8,
                  0xffa6dc9b,
                ])
                  IconButton.filled(
                    tooltip: '#${hex(Color(c))}',
                    style: IconButton.styleFrom(backgroundColor: Color(c)),
                    onPressed: saving
                        ? null
                        : () {
                            setState(() {
                              field.text = hex(Color(c));
                              dirty = true;
                            });
                            updateRoom();
                          },
                    icon: Icon(
                      Icons.check,
                      size: 18,
                      color: field.text == hex(Color(c))
                          ? Colors.black
                          : Colors.transparent,
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 12),
            if (part == 6 && g is Map)
              Text(
                [
                  if (state?['gpu'] != null) '${state!['gpu']} °C',
                  if (g['utilization'] != null) '${g['utilization']}%',
                  if (g['effect'] is String) g['effect'] as String,
                ].join('   ·   '),
              ),
            if (parseHex(field.text) case final draft?) ...[
              Row(
                children: [
                  const Text('색상'),
                  Expanded(
                    child: Slider(
                      value: HSVColor.fromColor(draft).hue.clamp(0, 359.9),
                      max: 359.9,
                      activeColor: draft,
                      onChanged: saving
                          ? null
                          : (v) {
                              final c = HSVColor.fromAHSV(1, v, 1, 1).toColor();
                              setState(() {
                                field.text = hex(c);
                                dirty = true;
                              });
                              updateRoom();
                            },
                    ),
                  ),
                ],
              ),
              Row(
                children: [
                  const Text('밝기'),
                  Expanded(
                    child: Slider(
                      value: HSVColor.fromColor(draft).value,
                      onChanged: saving
                          ? null
                          : (v) {
                              final c = basis.withValue(v).toColor();
                              setState(() {
                                field.text = hex(c);
                                dirty = true;
                              });
                              updateRoom();
                            },
                    ),
                  ),
                ],
              ),
            ],
            TextField(
              controller: field,
              maxLength: 6,
              enabled: !saving,
              textCapitalization: TextCapitalization.characters,
              decoration: const InputDecoration(
                labelText: '미리보기 색 · HEX',
                prefixText: '#',
              ),
              onChanged: (_) {
                setState(() => dirty = true);
                updateRoom();
              },
            ),
            FilledButton(
              onPressed: available && !saving && parseHex(field.text) != null
                  ? () => write(field.text.toUpperCase())
                  : null,
              child: Text(saving ? '저장 중…' : '색 저장'),
            ),
            TextButton(
              onPressed: available && !saving ? () => write(null) : null,
              child: const Text('효과 따르기'),
            ),
          ],
          if (error != null) Text(error!),
        ],
      ),
    );
  }
}
