import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:signals_flutter/signals_flutter.dart';
import 'package:yayma/src/features/core/views/widgets/lyrics_view.dart';
import 'package:yayma/src/features/core/views/widgets/download_menu.dart';
import 'package:yayma/src/features/playback/providers/lyrics_provider.dart';
import 'package:yayma/src/features/playback/providers/playback_provider.dart';
import 'package:yayma/src/rust/api/models.dart';

void main() {
  setUp(() {
    lyricsSourceSignal.value = 'auto';
    clearLyricsCache();
    playerProgressSignal.value = const PlaybackProgressDto(
      positionMs: 0,
      durationMs: 180000,
    );
    hideLyricsOverlaySignal.value = false;
  });

  test('cache invalidation notifies an already subscribed lyrics panel', () {
    final reads = <FutureSignal<LyricsResult>>[];
    final cleanup = effect(() {
      reads.add(lyricsSignal('cache-test'));
    });
    expect(reads.length, 1);
    clearLyricsCache();
    expect(reads.length, 2);
    expect(identical(reads[0], reads[1]), isFalse);
    lyricsSourceSignal.value = 'genius';
    expect(reads.length, 3);
    expect(identical(reads[1], reads[2]), isFalse);
    cleanup();
  });

  test('disabling the selected provider resets selection to automatic', () {
    lyricsSourceSignal.value = 'genius';
    applyLyricsProviderSettings(const [
      LyricsProviderSettingDto(id: 'lrclib', name: 'LRCLIB', enabled: true),
      LyricsProviderSettingDto(id: 'genius', name: 'Genius', enabled: false),
    ]);
    expect(enabledLyricsSourcesSignal.value, {'lrclib'});
    expect(lyricsSourceSignal.value, 'auto');
  });

  test('real timestamps control long lines and backward seeking', () {
    final items = toLyricItems(const [
      LyricsLineDto(startMs: 3000, text: 'First verse', words: []),
      LyricsLineDto(startMs: 20000, text: 'Second verse', words: []),
    ]);
    expect(items.length, 3); // intro, first line, second line
    expect(items.first, isA<LyricTimer>());
    expect(activeLyricIndex(items, 0), 0);
    expect(activeLyricIndex(items, 3000), 1);
    expect(activeLyricIndex(items, 19000), 1);
    expect(activeLyricIndex(items, 20000), 2);
    expect(activeLyricIndex(items, 179000), 2);
    expect(activeLyricIndex(items, 4000), 1);
  });

  Future<void> show(WidgetTester tester, LyricsResult result) async {
    final data = futureSignal<LyricsResult>(() async => result);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: LyricsWidget(
            trackId: 'widget-test',
            visible: true,
            resultSignal: data,
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 700));
  }

  testWidgets(
    'plain text stays visible even if the previous overlay was hidden',
    (tester) async {
      hideLyricsOverlaySignal.value = true;
      await show(
        tester,
        LyricsResult(
          [LyricLine(Duration.zero, 'Visible plain lyrics', Duration.zero)],
          'Genius',
          isSynced: false,
        ),
      );
      expect(find.text('Visible plain lyrics'), findsOneWidget);
      expect(find.textContaining('Без синхронизации'), findsOneWidget);
      expect(
        tester.getRect(find.text('Visible plain lyrics')).height,
        greaterThan(0),
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'a missing result shows a reason and retry instead of a blank panel',
    (tester) async {
      await show(tester, LyricsResult([], ''));
      expect(find.textContaining('Текст не найден'), findsOneWidget);
      expect(find.text('Повторить'), findsOneWidget);
    },
  );

  testWidgets('source menu shows only enabled providers', (tester) async {
    applyLyricsProviderSettings(const [
      LyricsProviderSettingDto(id: 'lrclib', name: 'LRCLIB', enabled: true),
      LyricsProviderSettingDto(id: 'genius', name: 'Genius', enabled: false),
    ]);
    await show(tester, LyricsResult([], ''));
    await tester.tap(find.byKey(const ValueKey('lyrics_source')));
    await tester.pumpAndSettle();
    expect(find.text('LRCLIB'), findsOneWidget);
    expect(find.text('Genius'), findsNothing);
    expect(find.byTooltip('Загрузить текст заново'), findsNothing);
  });

  testWidgets(
    'active line follows playback, seeks backward, and stays after the last timestamp',
    (tester) async {
      final items = toLyricItems(const [
        LyricsLineDto(startMs: 0, text: 'First verse', words: []),
        LyricsLineDto(startMs: 10000, text: 'Second verse', words: []),
      ]);
      await show(tester, LyricsResult(items, 'LRCLIB'));
      bool selected(int index) =>
          tester
              .widget<Semantics>(find.byKey(ValueKey('lyric_$index')))
              .properties
              .selected ??
          false;
      expect(selected(0), isTrue);
      playerProgressSignal.value = const PlaybackProgressDto(
        positionMs: 9650,
        durationMs: 180000,
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 700));
      expect(selected(1), isTrue);
      playerProgressSignal.value = const PlaybackProgressDto(
        positionMs: 2000,
        durationMs: 180000,
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 700));
      expect(selected(0), isTrue);
      playerProgressSignal.value = const PlaybackProgressDto(
        positionMs: 120000,
        durationMs: 180000,
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 700));
      expect(selected(1), isTrue);
      expect(hideLyricsOverlaySignal.value, isFalse);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('word timestamps never split or advance the displayed line', (
    tester,
  ) async {
    final items = toLyricItems(const [
      LyricsLineDto(
        startMs: 0,
        text: 'Complete first line',
        words: [LyricsWordDto(text: 'Wrong word text', startMs: 0, endMs: 500)],
      ),
      LyricsLineDto(startMs: 10000, text: 'Complete second line', words: []),
    ]);
    await show(tester, LyricsResult(items, 'YouLyPlus'));
    expect(find.text('Complete first line'), findsOneWidget);
    expect(find.text('Wrong word text'), findsNothing);
    expect(find.text('YouLyPlus · По строкам'), findsOneWidget);
    playerProgressSignal.value = const PlaybackProgressDto(
      positionMs: 5000,
      durationMs: 180000,
    );
    await tester.pump();
    expect(
      tester
          .widget<Semantics>(find.byKey(const ValueKey('lyric_0')))
          .properties
          .selected,
      isTrue,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'long synced lines wrap and remain centered at the search icon height',
    (tester) async {
      const longLine =
          'A very long lyric line which has enough words to wrap naturally across multiple lines without shrinking the text to an unreadable size';
      await show(
        tester,
        LyricsResult(
          toLyricItems(const [
            LyricsLineDto(startMs: 0, text: longLine, words: []),
            LyricsLineDto(startMs: 10000, text: 'Second line', words: []),
          ]),
          'YouLyPlus',
        ),
      );
      await tester.pump(const Duration(milliseconds: 700));
      await tester.pumpAndSettle();
      final first = tester.getRect(find.byKey(const ValueKey('lyric_0')));
      expect(first.height, greaterThan(110));
      expect(first.center.dy, closeTo(300, 2));
      playerProgressSignal.value = const PlaybackProgressDto(
        positionMs: 11000,
        durationMs: 180000,
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 700));
      await tester.pumpAndSettle();
      final second = tester.getRect(find.byKey(const ValueKey('lyric_1')));
      expect(second.center.dy, closeTo(300, 2));
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('last plain lyric scrolls above the bottom fade', (tester) async {
    await show(
      tester,
      LyricsResult([
        for (var i = 0; i < 30; i++)
          LyricLine(Duration.zero, 'Plain line $i', Duration.zero),
      ], 'Яндекс Музыка'),
    );
    final list = find.byType(ListView);
    await tester.drag(list, const Offset(0, -10000));
    await tester.pumpAndSettle();
    final viewport = tester.getRect(list);
    final last = tester.getRect(find.text('Plain line 29'));
    expect(last.top, greaterThan(viewport.top + viewport.height * .12));
    expect(last.bottom, lessThan(viewport.top + viewport.height * .72));
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'collection download uses the actual animated button as menu trigger',
    (tester) async {
      DownloadMode? chosen;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: DownloadTargetMenu(
                compact: false,
                isLoading: false,
                onSelected: (mode) => chosen = mode,
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Скачать'));
      await tester.pumpAndSettle();
      expect(find.text('В кэш приложения'), findsOneWidget);
      expect(find.text('В отдельные файлы'), findsOneWidget);
      await tester.tap(find.text('В отдельные файлы'));
      await tester.pumpAndSettle();
      expect(chosen, DownloadMode.files);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('source caption fades after two seconds without moving lyrics', (
    tester,
  ) async {
    await show(
      tester,
      LyricsResult([
        LyricLine(
          Duration.zero,
          'Stable lyric line',
          const Duration(seconds: 10),
        ),
      ], 'LRCLIB'),
    );
    final caption = find.text('LRCLIB · По строкам');
    final fade = find.ancestor(
      of: caption,
      matching: find.byType(AnimatedOpacity),
    );
    expect(tester.widget<AnimatedOpacity>(fade).opacity, 1);
    final before = tester.getRect(find.byKey(const ValueKey('lyric_0')));
    await tester.pump(const Duration(seconds: 2));
    await tester.pump(const Duration(milliseconds: 250));
    expect(tester.widget<AnimatedOpacity>(fade).opacity, 0);
    expect(tester.getRect(find.byKey(const ValueKey('lyric_0'))), before);
    expect(find.text('Stable lyric line'), findsOneWidget);
  });
}
