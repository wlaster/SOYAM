import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:signals_flutter/signals_flutter.dart';
import 'package:yayma/src/app/session.dart';
import 'package:yayma/src/features/core/services/debouncer.dart';
import 'package:yayma/src/rust/api/content.dart';
import 'package:yayma/src/rust/api/models.dart';

final FlutterSignal<String> searchQuerySignal = signal<String>('');

bool searchArtistsCollapsed = false;
bool searchAlbumsCollapsed = false;
File? _sectionPreferences;
Future<void> _sectionWrite = Future.value();

Future<void> loadSearchSections(String directory) async {
  _sectionPreferences = File(
    '$directory${Platform.pathSeparator}search-sections.json',
  );
  try {
    final data = jsonDecode(await _sectionPreferences!.readAsString());
    if (data is Map<String, dynamic>) {
      searchArtistsCollapsed = data['artistsCollapsed'] == true;
      searchAlbumsCollapsed = data['albumsCollapsed'] == true;
    }
  } on FileSystemException catch (_) {
  } on FormatException catch (_) {}
}

Future<void> saveSearchSections({
  required bool artistsCollapsed,
  required bool albumsCollapsed,
}) {
  searchArtistsCollapsed = artistsCollapsed;
  searchAlbumsCollapsed = albumsCollapsed;
  final data = jsonEncode({
    'artistsCollapsed': artistsCollapsed,
    'albumsCollapsed': albumsCollapsed,
  });
  return _sectionWrite = _sectionWrite.then((_) async {
    try {
      await _sectionPreferences?.writeAsString(data, flush: true);
    } on FileSystemException catch (error) {
      debugPrint('Search sections save failed: $error');
    }
  });
}

/// Search results signal (automatically updates when searchQuerySignal changes)
final FutureSignal<SearchResultsDto?> searchResultsSignal =
    futureSignal<SearchResultsDto?>(() async {
      final query = searchQuerySignal.value;
      if (query.trim().isEmpty) return null;

      final ctx = appContextSignal.value;
      if (ctx == null) return null;

      return await search(ctx: ctx, query: query);
    });

final Debouncer _searchDebouncer = Debouncer();

void cancelSearchDebounce() {
  _searchDebouncer.cancel();
}

void setSearchQuery(String query) {
  _searchDebouncer.run(() {
    searchQuerySignal.value = query;
  }, const Duration(milliseconds: 300));
}

Future<void> disposeSearch() async {
  _searchDebouncer.dispose();
  searchQuerySignal.value = '';
}
