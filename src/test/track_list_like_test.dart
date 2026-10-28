import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:yayma/src/features/core/views/widgets/common_ui.dart';
import 'package:yayma/src/features/core/views/widgets/track_tile.dart';
import 'package:yayma/src/features/playback/providers/playback_provider.dart';

import 'player_controls_test.dart' as fixtures;

void main() {
  testWidgets('album playing status replaces the centered number', (
    tester,
  ) async {
    playerStateSignal.value = fixtures.state(playing: true);
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: Center(
            child: CommonTrackTile(
              trackId: '1',
              title: 'Track',
              artists: [],
              replaceLeadingWhenPlaying: true,
              durationMs: 60000,
              leading: SizedBox(
                key: ValueKey('number-slot'),
                width: 32,
                height: 48,
                child: Center(child: Text('1')),
              ),
            ),
          ),
        ),
      ),
    );
    final hiddenNumber = tester.widget<Visibility>(
      find
          .ancestor(of: find.text('1'), matching: find.byType(Visibility))
          .first,
    );
    expect(hiddenNumber.visible, isFalse);
    expect(
      tester.getCenter(find.byKey(const ValueKey('number-slot'))).dy,
      closeTo(tester.getCenter(find.text('1:00')).dy, 0.1),
    );
    playerStateSignal.value = null;
    await tester.pumpAndSettle();
    expect(
      find.ancestor(of: find.text('1'), matching: find.byType(Visibility)),
      findsNothing,
    );
  });

  tearDown(() {
    playerStateSignal.value = null;
    trackLikeOverridesSignal.value = {};
    pendingTrackLikesSignal.value = {};
  });
  testWidgets(
    'like precedes duration, centers on hover and has no background',
    (tester) async {
      tester.view.physicalSize = const Size(1200, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      var plays = 0;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: CommonTrackTile(
                trackId: 'original',
                title: 'ON MY MIND',
                artists: const [],
                durationMs: 60000,
                leading: const SizedBox(
                  width: 32,
                  height: 48,
                  child: Center(child: Text('1')),
                ),
                hoverActions: [
                  IconButton(onPressed: () {}, icon: const Icon(Icons.add)),
                ],
                onTap: () => plays++,
              ),
            ),
          ),
        ),
      );
      final heart = find.byKey(const ValueKey('track-like-original'));
      final duration = find.text('1:00');
      final before = tester.getCenter(duration);
      expect(tester.getCenter(heart).dx, lessThan(before.dx));
      expect(tester.getCenter(heart).dy, closeTo(before.dy, 0.1));
      expect(tester.getCenter(find.text('1')).dy, closeTo(before.dy, 0.1));
      expect(tester.widget<AnimatedLikeButton>(heart).showBackground, isFalse);
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset.zero);
      await mouse.moveTo(tester.getCenter(heart));
      await tester.pumpAndSettle();
      expect(tester.getCenter(duration), before);
      final ink = find.descendant(of: heart, matching: find.byType(InkWell));
      expect(tester.widget<InkWell>(ink).hoverColor, Colors.transparent);
      pendingTrackLikesSignal.value = {'original'};
      await tester.pump();
      await tester.tap(heart);
      await tester.pump();
      expect(plays, 0);
      await mouse.removePointer();
    },
  );
  testWidgets('like follows track ID across rows, preserving another version', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Column(
            children: [
              for (final id in ['original', 'original:album', 'sped-up'])
                CommonTrackTile(
                  key: ValueKey(id),
                  trackId: id,
                  title: 'ON MY MIND',
                  artists: const [],
                  durationMs: 60000,
                ),
            ],
          ),
        ),
      ),
    );
    trackLikeOverridesSignal.value = {'original': true};
    await tester.pumpAndSettle();
    final hearts = tester
        .widgetList<AnimatedLikeButton>(find.byType(AnimatedLikeButton))
        .toList();
    expect(hearts.map((heart) => heart.isLiked), [true, true, false]);
    expect(tester.takeException(), isNull);
  });
  test('failed like restores state and releases pending guard', () async {
    trackLikeOverridesSignal.value = {'original': false};
    final confirmed = await PlaybackController.toggleLike(trackId: 'original');
    expect(confirmed, isFalse);
    expect(trackLikeOverridesSignal.value['original'], isFalse);
    expect(pendingTrackLikesSignal.value, isEmpty);
  });
}
