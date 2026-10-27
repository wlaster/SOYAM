import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/views/widgets/common_ui.dart';
import 'package:yayma/src/features/playback/providers/playback_provider.dart';
import 'package:yayma/src/rust/api/models.dart';

PlaybackState state({
  int volume = 50,
  bool playing = false,
  bool wave = false,
}) => PlaybackState(
  isPlaying: playing,
  isBuffering: false,
  volume: volume,
  isMuted: false,
  repeatMode: RepeatModeDto.none,
  isShuffled: false,
  queueCount: 2,
  queueIndex: 0,
  currentWaveSeeds: wave ? ['track:1'] : [],
  currentTrack: const SimpleTrackDto(
    id: '1',
    title: 'Track',
    artists: [],
    durationMs: 1000,
    isLiked: false,
    isDisliked: false,
  ),
);

void main() {
  tearDown(() {
    playerStateSignal.value = null;
    activeCollectionSignal.value = null;
  });

  testWidgets('collection play button highlights only its playing collection', (
    tester,
  ) async {
    var starts = 0;
    playerStateSignal.value = state();
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: CollectionPlayButton(
            collectionKey: 'album:1',
            onPlay: () async {
              starts++;
            },
          ),
        ),
      ),
    );
    expect(find.byIcon(Icons.play_arrow_rounded), findsOneWidget);
    final offColor = tester
        .widget<IconButton>(find.byType(IconButton))
        .style!
        .backgroundColor!
        .resolve({});
    await tester.tap(find.byType(IconButton));
    expect(starts, 1);
    activeCollectionSignal.value = 'album:1';
    playerStateSignal.value = state(playing: true);
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.pause_rounded), findsOneWidget);
    final onColor = tester
        .widget<IconButton>(find.byType(IconButton))
        .style!
        .backgroundColor!
        .resolve({});
    expect(onColor, isNot(offColor));
    await tester.tap(find.byType(IconButton));
    expect(starts, 1); // Pause/resume must not reload the collection.
    playerStateSignal.value = state();
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.play_arrow_rounded), findsOneWidget);
    playerStateSignal.value = state(playing: true, wave: true);
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.play_arrow_rounded), findsOneWidget);
    activeCollectionSignal.value = 'playlist:2:3';
    playerStateSignal.value = state(playing: true);
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.play_arrow_rounded), findsOneWidget);
  });

  testWidgets(
    'volume wheel accumulates small movements and clamps at both ends',
    (tester) async {
      final values = <int>[];
      playerStateSignal.value = state();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: VolumeScrollRegion(
                onChanged: values.add,
                child: const Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [VolumeStateIcon(), CommonVolumeSlider(width: 240)],
                ),
              ),
            ),
          ),
        ),
      );
      final position = tester.getCenter(find.byType(VolumeScrollRegion));
      Future<void> scroll(double delta) async {
        await tester.sendEventToBinding(
          PointerScrollEvent(position: position, scrollDelta: Offset(0, delta)),
        );
      }

      await scroll(120);
      expect(values.last, 47);
      await tester.pump();
      expect(find.text('47'), findsOneWidget);
      final label = find.byKey(const ValueKey('wheel-volume-label'));
      final firstX = tester.getCenter(label).dx;
      await scroll(-120);
      expect(values.last, 50);
      await tester.pump();
      expect(find.text('50'), findsOneWidget);
      expect(tester.getCenter(label).dx, greaterThan(firstX));
      await scroll(10);
      await scroll(10);
      await scroll(10);
      expect(values.last, 49);
      await scroll(10000);
      expect(values.last, 0);
      await scroll(-10000);
      expect(values.last, 100);
      await tester.pump();
      expect(find.text('100'), findsOneWidget);
      await tester.pump(const Duration(milliseconds: 700));
      expect(find.byKey(const ValueKey('wheel-volume-label')), findsNothing);
    },
  );
}
