import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/providers/visual_effects_provider.dart';
import 'package:yayma/src/features/core/views/widgets/progress_appearance_slider.dart';
import 'package:yayma/src/features/library/providers/library_provider.dart';
import 'package:yayma/src/rust/api/models.dart';

void main() {
  test(
    'deleted playlist stays removed after stale refresh, other owner remains',
    () async {
      const own = SimplePlaylistDto(
        kind: 5,
        uid: 1,
        title: 'Own',
        trackCount: 1,
        isPublic: false,
      );
      const other = SimplePlaylistDto(
        kind: 5,
        uid: 2,
        title: 'Other',
        trackCount: 1,
        isPublic: true,
      );
      reconcilePlaylists([own, other]);
      applyPlaylistDeletion(1, 5);
      expect(playlistsSignal.value, [other]);
      reconcilePlaylists([own, other]);
      expect(playlistsSignal.value, [other]);
      await disposeLibrary();
    },
  );
  test('appearance persists across loads', () async {
    final directory = Directory.systemTemp.createTempSync('soyam-progress-');
    try {
      await loadProgressAppearance(directory.path);
      await setProgressAppearance(ProgressAppearance.visualizer);
      progressAppearanceSignal.value = ProgressAppearance.standard;
      await loadProgressAppearance(directory.path);
      expect(progressAppearanceSignal.value, ProgressAppearance.visualizer);
    } finally {
      directory.deleteSync(recursive: true);
      progressAppearanceSignal.value = ProgressAppearance.standard;
    }
  });
  for (final style in [
    ProgressAppearance.android,
    ProgressAppearance.visualizer,
  ]) {
    testWidgets('$style retains seeking and paints without overflow', (
      tester,
    ) async {
      double? value;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: SizedBox(
                width: 300,
                child: ProgressAppearanceSlider(
                  appearance: style,
                  value: 20,
                  max: 100,
                  color: Colors.blue,
                  inactiveColor: Colors.grey,
                  onStart: (_) {},
                  onChanged: (v) => value = v,
                  onEnd: (_) {},
                ),
              ),
            ),
          ),
        ),
      );
      await tester.tapAt(
        tester.getCenter(find.byType(Slider)) + const Offset(65, 0),
      );
      await tester.pumpAndSettle();
      expect(value, greaterThan(50));
      expect(tester.takeException(), isNull);
    });
  }
}
