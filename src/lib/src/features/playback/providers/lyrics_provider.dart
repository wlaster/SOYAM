import 'package:signals_flutter/signals_flutter.dart';
import 'package:yayma/src/features/core/services/rust_bridge.dart';
import 'package:yayma/src/features/core/utils/lru_map.dart';
import 'package:yayma/src/rust/api/content.dart';
import 'package:yayma/src/rust/api/models.dart';

sealed class LyricItem {
  final Duration time;
  LyricItem(this.time);
}

class LyricLine extends LyricItem {
  final String text;
  final Duration duration;
  LyricLine(super.time, this.text, this.duration);
}

class LyricTimer extends LyricItem {
  final Duration duration;
  LyricTimer(super.time, this.duration);
}

/// A track's parsed lyrics plus the name of the source that provided them,
/// so the UI can show where the text came from.
class LyricsResult {
  final List<LyricItem> items;
  final String providerName;
  final bool isSynced;
  LyricsResult(this.items, this.providerName, {this.isSynced = true});
}

const lyricsSources = <String, String>{
  'auto': 'Авто · с синхронизацией',
  'yandex': 'Яндекс Музыка',
  'lrclib': 'LRCLIB',
  'lyricsovh': 'Lyrics.ovh',
  'genius': 'Genius',
  'betterlyrics': 'BetterLyrics',
  'youlyplus': 'YouLyPlus',
};
final FlutterSignal<String> lyricsSourceSignal = signal<String>('auto');
final FlutterSignal<Set<String>> enabledLyricsSourcesSignal = signal(
  <String>{},
);

void applyLyricsProviderSettings(List<LyricsProviderSettingDto> settings) {
  enabledLyricsSourcesSignal.value = {
    for (final provider in settings)
      if (provider.enabled) provider.id,
  };
  if (lyricsSourceSignal.value != 'auto' &&
      !enabledLyricsSourcesSignal.value.contains(lyricsSourceSignal.value)) {
    lyricsSourceSignal.value = 'auto';
  }
  clearLyricsCache();
}

Future<void> refreshLyricsProviderSettings() async {
  final settings = await runRustFetch(
    (ctx) => getLyricsProviderSettings(ctx: ctx),
    notifyErrors: false,
  );
  if (settings != null) applyLyricsProviderSettings(settings);
}

final FlutterSignal<int> _lyricsRevision = signal<int>(0);

const int _maxLyricsCacheSize = 20;

/// Bounded LRU of per-track lyrics fetches. Reads promote entries and
/// inserts past capacity evict the least-recently-used track (see [LruMap]).
final LruMap<String, FutureSignal<LyricsResult>> _lyricsCache = LruMap(
  maximumSize: _maxLyricsCacheSize,
);

/// Drops every cached lyrics fetch so the next [lyricsSignal] read refetches
/// from Rust. Call this when the set of enabled lyrics providers changes —
/// otherwise a track's lyrics stay pinned to whatever source answered
/// before the toggle, even after a source is disabled.
/// Also call from logout to avoid leaking per-user data.
void clearLyricsCache() {
  _lyricsCache.clear();
  _lyricsRevision.value++;
}

void retryLyrics(String trackId) {
  _lyricsCache.remove('${lyricsSourceSignal.value}|$trackId');
  _lyricsRevision.value++;
}

FutureSignal<LyricsResult> lyricsSignal(String trackId) {
  _lyricsRevision.value;
  final source = lyricsSourceSignal.value;
  final cacheKey = '$source|$trackId';
  final existing = _lyricsCache[cacheKey];
  if (existing != null) return existing;
  final created = futureSignal<LyricsResult>(() async {
    final result = await runRustFetch(
      (ctx) => getLyrics(
        ctx: ctx,
        trackId: trackId,
        source: source == 'auto' ? null : source,
      ),
    );
    if (result == null) return LyricsResult([], '');
    return LyricsResult(
      toLyricItems(result.lines),
      result.providerName,
      isSynced: result.lines.any((line) => line.startMs >= 0),
    );
  });
  _lyricsCache[cacheKey] = created;
  return created;
}

List<LyricItem> toLyricItems(List<LyricsLineDto> lines) {
  if (lines.isEmpty) return [];
  if (lines.every((line) => line.startMs < 0)) {
    return lines
        .map((line) => LyricLine(Duration.zero, line.text, Duration.zero))
        .toList();
  }

  final rawLines = lines
      .map(
        (line) => (time: Duration(milliseconds: line.startMs), text: line.text),
      )
      .toList();

  final result = <LyricItem>[];

  if (rawLines.first.time > Duration.zero) {
    result.add(LyricTimer(Duration.zero, rawLines.first.time));
  }
  for (var i = 0; i < rawLines.length; i++) {
    final current = rawLines[i];
    final nextTime = i + 1 < rawLines.length
        ? rawLines[i + 1].time
        : current.time + const Duration(seconds: 10);
    if (current.text.trim().isEmpty) {
      result.add(LyricTimer(current.time, nextTime - current.time));
    } else {
      result.add(
        LyricLine(current.time, current.text, nextTime - current.time),
      );
    }
  }
  return result;
}

/// Binary search against the playback clock; also handles backward seeks.
int activeLyricIndex(List<LyricItem> items, int positionMs) {
  var lo = 0;
  var hi = items.length;
  while (lo < hi) {
    final mid = (lo + hi) >> 1;
    if (items[mid].time.inMilliseconds > positionMs) {
      hi = mid;
    } else {
      lo = mid + 1;
    }
  }
  return lo - 1;
}
