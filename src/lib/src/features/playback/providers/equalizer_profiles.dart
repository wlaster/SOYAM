import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;

import 'package:path_provider/path_provider.dart';

class EqualizerProfile {
  EqualizerProfile(
    this.id,
    this.name,
    List<double> gains, {
    this.builtIn = false,
  }) : gains = List.unmodifiable(gains);

  final String id;
  final String name;
  final List<double> gains;
  final bool builtIn;
}

/// Ten-band curves interpolated logarithmically to the player's 15 bands.
List<double> _curve(List<double> values) {
  const frequencies = [
    25,
    40,
    63,
    100,
    160,
    250,
    400,
    630,
    1000,
    1600,
    2500,
    4000,
    6300,
    10000,
    16000,
  ];
  return frequencies.map((frequency) {
    final position = (math.log(frequency / 31.25) / math.ln2).clamp(0.0, 9.0);
    final lower = position.floor();
    final upper = math.min(lower + 1, 9);
    return values[lower] + (values[upper] - values[lower]) * (position - lower);
  }).toList();
}

final standardEqualizerProfiles = List<EqualizerProfile>.unmodifiable(
  <String, List<double>>{
    'Ровный': [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    'Классика': [0, 0, 0, 0, 0, 0, -3, -3, -3, -4],
    'Клуб': [0, 0, 2, 4, 4, 4, 2, 0, 0, 0],
    'Танцевальная': [5, 4, 2, 0, 0, -2, -3, -3, 0, 0],
    'Усиление басов': [6, 5, 4, 3, 1, 0, 0, 0, 0, 0],
    'Усиление высоких': [0, 0, 0, 0, 0, 1, 3, 4, 5, 6],
    'Басы и высокие': [4, 3, 1, -2, -3, -3, -1, 2, 4, 5],
    'Наушники': [3, 4, 2, 0, -2, -1, 1, 3, 4, 5],
    'Большой зал': [4, 4, 2, 2, 0, -2, -2, -2, 0, 0],
    'Концерт': [-2, 0, 2, 3, 3, 3, 2, 1, 1, 1],
    'Вечеринка': [4, 4, 0, 0, 0, 0, 0, 0, 4, 4],
    'Поп': [-1, 2, 3, 4, 2, 0, -1, -1, -1, -1],
    'Регги': [0, 0, 0, -2, 0, 3, 3, 0, 0, 0],
    'Рок': [4, 3, -3, -4, -2, 2, 4, 5, 5, 5],
    'Ска': [-1, -2, -2, 0, 2, 3, 4, 4, 5, 4],
    'Мягкий': [2, 1, 0, -1, 0, 2, 3, 4, 5, 5],
    'Мягкий рок': [2, 2, 1, 0, -2, -3, -1, 0, 1, 4],
    'Техно': [4, 3, 0, -3, -3, 0, 4, 5, 5, 4],
    'Электронная': [4, 3, 0, -2, -1, 2, 1, 2, 4, 5],
    'Джаз': [3, 2, 1, 2, -2, -2, 0, 1, 2, 3],
    'Хип-хоп': [5, 4, 1, 2, -1, -1, 1, 0, 2, 3],
    'Акустика': [4, 3, 2, 1, 1, 2, 3, 3, 2, 1],
    'Вокал': [-3, -3, -2, 0, 3, 4, 3, 1, 0, -2],
    'Речь / подкасты': [-4, -3, -2, 1, 3, 4, 3, 1, -1, -3],
    'Лаунж': [-2, 0, 2, 3, 2, 0, -1, 1, 2, 1],
  }.entries.map(
    (entry) => EqualizerProfile(
      'standard:${entry.key}',
      entry.key,
      _curve(entry.value),
      builtIn: true,
    ),
  ),
);

class EqualizerProfiles {
  EqualizerProfiles({File? file}) : _file = file;

  static int _lastId = 0;
  File? _file;
  Future<void> _writes = Future.value();
  final List<EqualizerProfile> _custom = [];
  String? selectedId;
  List<EqualizerProfile> get custom => List.unmodifiable(_custom);
  List<EqualizerProfile> get all => [...standardEqualizerProfiles, ..._custom];
  bool get canCreate => _custom.length < 4;

  Future<File> _resolveFile() async => _file ??= File(
    '${(await getApplicationSupportDirectory()).path}/equalizer_profiles.json',
  );

  Future<void> load() async {
    final file = await _resolveFile();
    if (!file.existsSync()) return;
    final data = jsonDecode(await file.readAsString()) as Map<String, dynamic>;
    final profiles = <EqualizerProfile>[];
    for (final raw in (data['profiles'] as List).take(4)) {
      final item = raw as Map<String, dynamic>;
      final gains = (item['gains'] as List).cast<num>();
      if (gains.length != 15 || gains.any((gain) => !gain.isFinite)) continue;
      final id = item['id'] as String;
      if (!id.startsWith('custom:') || profiles.any((p) => p.id == id)) {
        continue;
      }
      profiles.add(
        EqualizerProfile(
          id,
          item['name'] as String,
          gains.map((gain) => gain.toDouble().clamp(-12.0, 12.0)).toList(),
        ),
      );
    }
    _custom
      ..clear()
      ..addAll(profiles);
    selectedId = data['selected'] as String?;
  }

  EqualizerProfile create(String name, List<double> gains) {
    if (!canCreate) {
      throw StateError('Можно создать не больше 4 своих профилей');
    }
    _lastId = math.max(_lastId + 1, DateTime.now().microsecondsSinceEpoch);
    final profile = EqualizerProfile('custom:$_lastId', name.trim(), gains);
    _custom.add(profile);
    selectedId = profile.id;
    return profile;
  }

  void update(String id, List<double> gains, {String? name}) {
    final index = _custom.indexWhere((profile) => profile.id == id);
    if (index < 0) throw StateError('Стандартные профили нельзя менять');
    final current = _custom[index];
    _custom[index] = EqualizerProfile(id, name?.trim() ?? current.name, gains);
  }

  void copyTo(EqualizerProfile source, String destinationId) =>
      update(destinationId, source.gains);

  void remove(String id) {
    _custom.removeWhere((profile) => profile.id == id);
    if (selectedId == id) selectedId = null;
  }

  Future<void> save() {
    final snapshot = jsonEncode({
      'selected': selectedId,
      'profiles': _custom
          .map(
            (profile) => {
              'id': profile.id,
              'name': profile.name,
              'gains': profile.gains,
            },
          )
          .toList(),
    });
    final write = _writes.then((_) async {
      final file = await _resolveFile();
      await file.parent.create(recursive: true);
      final temporary = File('${file.path}.tmp');
      await temporary.writeAsString(snapshot, flush: true);
      await temporary.rename(file.path);
    });
    _writes = write.catchError((Object _) {});
    return write;
  }
}
