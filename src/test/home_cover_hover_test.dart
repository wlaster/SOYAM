import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/views/widgets/home_cover_widget.dart';
import 'package:yayma/src/features/playback/providers/playback_provider.dart';
import 'package:yayma/src/rust/api/models.dart';

void main() {
  testWidgets('home cover fades the track-wave overlay in and out on hover', (
    tester,
  ) async {
    playerStateSignal.value = const PlaybackState(
      isPlaying: false,
      isBuffering: false,
      volume: 50,
      isMuted: false,
      repeatMode: RepeatModeDto.none,
      isShuffled: false,
      queueCount: 1,
      queueIndex: 0,
      currentWaveSeeds: [],
      currentTrack: SimpleTrackDto(
        id: '1',
        title: 'Track',
        artists: [],
        durationMs: 1000,
        isLiked: false,
        isDisliked: false,
        isExplicit: false,
        isUnavailable: false,
      ),
    );
    addTearDown(() => playerStateSignal.value = null);
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(body: Center(child: HomeCoverWidget())),
      ),
    );
    await tester.pumpAndSettle();
    final overlay = find.byKey(const ValueKey('home-cover-wave-hover'));
    expect(tester.widget<AnimatedOpacity>(overlay).opacity, 0);
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(tester.getCenter(find.byType(HomeCoverWidget)));
    await tester.pumpAndSettle();
    expect(tester.widget<AnimatedOpacity>(overlay).opacity, 1);
    expect(find.byIcon(Icons.waves_rounded), findsOneWidget);
    expect(find.text('Волна по треку · двойной клик'), findsOneWidget);
    await tester.tap(find.byType(HomeCoverWidget));
    await tester.pump(const Duration(milliseconds: 350));
    expect(find.text('Волна активирована'), findsNothing);
    await tester.tap(find.byType(HomeCoverWidget));
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tap(find.byType(HomeCoverWidget));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.text('Волна активирована'), findsOneWidget);
    await tester.pump(const Duration(seconds: 3));
    await mouse.moveTo(Offset.zero);
    await tester.pumpAndSettle();
    expect(tester.widget<AnimatedOpacity>(overlay).opacity, 0);
    await mouse.removePointer();
    expect(tester.takeException(), isNull);
  });
}
