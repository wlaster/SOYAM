import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:yayma/src/features/playback/providers/equalizer_profiles.dart';

void main() {
  test('standard profiles cover all 15 bands and cannot be edited', () {
    final profiles = EqualizerProfiles();
    expect(standardEqualizerProfiles.length, greaterThanOrEqualTo(20));
    for (final preset in standardEqualizerProfiles) {
      expect(preset.builtIn, isTrue);
      expect(preset.gains.length, 15);
      expect(
        preset.gains.every((v) => v.isFinite && v >= -12 && v <= 12),
        isTrue,
      );
      expect(
        () => profiles.update(preset.id, List.filled(15, 1)),
        throwsStateError,
      );
      expect(() => preset.gains[0] = 8, throwsUnsupportedError);
    }
  });

  test(
    'copies are independent, only four custom slots, deleting frees a slot',
    () {
      final profiles = EqualizerProfiles();
      final rock = standardEqualizerProfiles.firstWhere((p) => p.name == 'Рок');
      final first = profiles.create('Рок — свой', rock.gains);
      final second = profiles.create('Второй', List.filled(15, 0));
      profiles.copyTo(first, second.id);
      expect(profiles.custom[1].gains, first.gains);
      profiles.update(first.id, List.filled(15, -2));
      expect(profiles.custom[1].gains, rock.gains);
      expect(rock.gains, isNot(List.filled(15, -2)));
      profiles
        ..create('Третий', rock.gains)
        ..create('Четвёртый', rock.gains);
      expect(profiles.canCreate, isFalse);
      expect(() => profiles.create('Пятый', rock.gains), throwsStateError);
      profiles.remove(second.id);
      expect(profiles.canCreate, isTrue);
      profiles.create('Замена', rock.gains);
      expect(profiles.custom.length, 4);
      expect(profiles.custom.map((p) => p.id).toSet().length, 4);
    },
  );

  test('settings, names and selection survive saving and reopening', () async {
    final directory = Directory.systemTemp.createTempSync('soyam-eq-test-');
    addTearDown(() => directory.deleteSync(recursive: true));
    final file = File('${directory.path}/profiles.json');
    final profiles = EqualizerProfiles(file: file);
    final first = profiles.create('Мой', List.filled(15, 2));
    final initialSave = profiles.save();
    profiles.update(first.id, List.filled(15, 3), name: 'Переименован');
    final updatedSave = profiles.save();
    await Future.wait([initialSave, updatedSave]);
    final restored = EqualizerProfiles(file: file);
    await restored.load();
    expect(restored.selectedId, first.id);
    expect(restored.custom.single.name, 'Переименован');
    expect(restored.custom.single.gains, List.filled(15, 3));
    restored.remove(first.id);
    await restored.save();
    final empty = EqualizerProfiles(file: file);
    await empty.load();
    expect(empty.custom, isEmpty);
    expect(empty.selectedId, isNull);
  });
}
