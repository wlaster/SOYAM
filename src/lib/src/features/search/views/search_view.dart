import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:signals_flutter/signals_flutter.dart';
import 'package:yayma/src/features/core/providers/navigation_provider.dart';
import 'package:yayma/src/features/core/views/widgets/app_cover.dart';
import 'package:yayma/src/features/core/views/widgets/common_ui.dart';
import 'package:yayma/src/features/core/views/widgets/horizontal_shelf.dart';
import 'package:yayma/src/features/core/views/widgets/media_card.dart';
import 'package:yayma/src/features/core/views/widgets/responsive.dart';
import 'package:yayma/src/features/core/views/widgets/smooth_scroll.dart';
import 'package:yayma/src/features/core/views/widgets/tab_visibility.dart';
import 'package:yayma/src/features/core/views/widgets/track_list.dart';
import 'package:yayma/src/features/core/views/widgets/track_tile.dart';
import 'package:yayma/src/features/playback/providers/playback_provider.dart';
import 'package:yayma/src/features/search/providers/search_provider.dart';
import 'package:yayma/src/rust/api/models.dart';

class SearchView extends StatefulWidget {
  const SearchView({super.key});

  @override
  State<SearchView> createState() => _SearchViewState();
}

class _SearchViewState extends State<SearchView> {
  final TextEditingController _controller = TextEditingController();
  final _scrollController = SmoothScrollController(debugLabel: 'search');
  EffectCleanup? _refreshSub;
  var _lastRefreshSeen = 0;

  @override
  void initState() {
    super.initState();
    _controller.text = searchQuerySignal.value;
    _lastRefreshSeen = refreshRequestSignal.peek();
    _refreshSub = refreshRequestSignal.subscribe((count) {
      if (count == _lastRefreshSeen) return;
      _lastRefreshSeen = count;
      if (currentNavStateSignal.peek().section == AppSection.search) {
        unawaited(searchResultsSignal.refresh());
      }
    });
  }

  @override
  void dispose() {
    _refreshSub?.call();
    _controller.dispose();
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return SignalBuilder(
      builder: (context) {
        // No subscription while the tab is hidden; results arriving in
        // the background don't rebuild it, they are picked up on open.
        final searchResultsAsync = visibleSignal(context, searchResultsSignal);
        final screenWidth = MediaQuery.sizeOf(context).width;
        final isNarrow = screenWidth < 600;

        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: isNarrow
                  ? const EdgeInsets.fromLTRB(20, 16, 20, 8)
                  : context.viewPadding.copyWith(top: 40, bottom: 0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const CommonPageTitle(title: 'Поиск'),
                  if (!isNarrow) const SizedBox(height: 24),
                  if (isNarrow) const SizedBox(height: 8),
                  TextField(
                    controller: _controller,
                    onChanged: setSearchQuery,
                    style: TextStyle(
                      fontSize: isNarrow ? 18 : 24,
                      color: Theme.of(context).colorScheme.onSurface,
                    ),
                    decoration: InputDecoration(
                      hintText: 'Треки, альбомы, артисты...',
                      hintStyle: TextStyle(
                        color: Theme.of(
                          context,
                        ).colorScheme.onSurfaceVariant.withValues(alpha: 0.6),
                      ),
                      prefixIcon: Padding(
                        padding: EdgeInsets.symmetric(
                          horizontal: isNarrow ? 12 : 16,
                        ),
                        child: Icon(
                          Icons.search,
                          color: Theme.of(context).colorScheme.onSurfaceVariant,
                          size: isNarrow ? 24 : 32,
                        ),
                      ),
                      filled: true,
                      fillColor: Theme.of(
                        context,
                      ).colorScheme.onSurface.withValues(alpha: 0.05),
                      border: OutlineInputBorder(
                        borderRadius: BorderRadius.circular(isNarrow ? 16 : 24),
                        borderSide: BorderSide.none,
                      ),
                      contentPadding: EdgeInsets.symmetric(
                        vertical: isNarrow ? 16 : 24,
                        horizontal: 20,
                      ),
                    ),
                  ),
                ],
              ),
            ),
            Expanded(
              child: searchResultsAsync.map(
                data: (results) {
                  if (results == null) return const _EmptySearchState();
                  if (results.tracks.isEmpty &&
                      results.albums.isEmpty &&
                      results.artists.isEmpty) {
                    return AppRefreshIndicator(
                      onRefresh: searchResultsSignal.refresh,
                      child: CustomScrollView(
                        controller: _scrollController,
                        physics: const AlwaysScrollableScrollPhysics(),
                        slivers: [
                          SliverFillRemaining(
                            hasScrollBody: false,
                            child: Center(
                              child: Text(
                                'Ничего не найдено',
                                style: TextStyle(
                                  color: Theme.of(
                                    context,
                                  ).colorScheme.onSurfaceVariant,
                                  fontSize: 18,
                                ),
                              ),
                            ),
                          ),
                        ],
                      ),
                    );
                  }
                  return _SearchResults(
                    results: results,
                    controller: _scrollController,
                  );
                },
                loading: () => const CommonLoadingWidget(),
                error: (Object e, StackTrace? _) =>
                    CommonErrorWidget(error: e.toString()),
              ),
            ),
          ],
        );
      },
    );
  }
}

class _EmptySearchState extends StatelessWidget {
  const _EmptySearchState();

  @override
  Widget build(BuildContext context) {
    final onSurface = Theme.of(context).colorScheme.onSurface;
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(
            Icons.manage_search_rounded,
            size: 80,
            color: onSurface.withValues(alpha: 0.1),
          ),
          const SizedBox(height: 16),
          Text(
            'Начните вводить текст',
            style: TextStyle(
              color: onSurface.withValues(alpha: 0.38),
              fontSize: 18,
            ),
          ),
        ],
      ),
    );
  }
}

class _SearchResults extends StatefulWidget {
  final SearchResultsDto results;
  final ScrollController controller;

  const _SearchResults({required this.results, required this.controller});

  @override
  State<_SearchResults> createState() => _SearchResultsState();
}

class _SearchResultsState extends State<_SearchResults> {
  bool _artistsCollapsed = searchArtistsCollapsed;
  bool _albumsCollapsed = searchAlbumsCollapsed;

  void _toggleSection({required bool artists}) {
    setState(() {
      if (artists) {
        _artistsCollapsed = !_artistsCollapsed;
      } else {
        _albumsCollapsed = !_albumsCollapsed;
      }
    });
    unawaited(
      saveSearchSections(
        artistsCollapsed: _artistsCollapsed,
        albumsCollapsed: _albumsCollapsed,
      ),
    );
  }

  SearchResultsDto get results => widget.results;
  ScrollController get controller => widget.controller;

  Widget _sectionHeader(
    String title,
    bool collapsed,
    VoidCallback onPressed, {
    double top = 24,
  }) {
    return Padding(
      padding: EdgeInsets.fromLTRB(
        context.horizontalPadding,
        top,
        context.horizontalPadding,
        24,
      ),
      child: Row(
        children: [
          CommonSectionTitle(title: title, padding: EdgeInsets.zero),
          const SizedBox(width: 8),
          IconButton(
            onPressed: onPressed,
            tooltip: collapsed ? 'Развернуть' : 'Свернуть',
            icon: AnimatedRotation(
              turns: collapsed ? 0.5 : 0,
              duration: MediaQuery.disableAnimationsOf(context)
                  ? Duration.zero
                  : const Duration(milliseconds: 300),
              curve: Curves.easeInOutCubic,
              child: const Icon(Icons.expand_less_rounded),
            ),
            padding: EdgeInsets.zero,
            constraints: const BoxConstraints.tightFor(width: 32, height: 32),
            style: const ButtonStyle(
              tapTargetSize: MaterialTapTargetSize.shrinkWrap,
              shape: WidgetStatePropertyAll(
                RoundedRectangleBorder(
                  borderRadius: BorderRadius.all(Radius.circular(12)),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _animatedShelf({required bool collapsed, required Widget child}) {
    return SliverToBoxAdapter(
      child: IgnorePointer(
        ignoring: collapsed,
        child: ExcludeFocus(
          excluding: collapsed,
          child: ExcludeSemantics(
            excluding: collapsed,
            child: TweenAnimationBuilder<double>(
              tween: Tween<double>(begin: 1, end: collapsed ? 0 : 1),
              duration: MediaQuery.disableAnimationsOf(context)
                  ? Duration.zero
                  : const Duration(milliseconds: 300),
              curve: Curves.easeInOutCubic,
              builder: (context, value, child) => ClipRect(
                child: Align(
                  alignment: Alignment.topCenter,
                  heightFactor: value,
                  child: Opacity(opacity: value, child: child),
                ),
              ),
              child: child,
            ),
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return AppRefreshIndicator(
      onRefresh: searchResultsSignal.refresh,
      child: CustomScrollView(
        controller: controller,
        physics: const AlwaysScrollableScrollPhysics(),
        slivers: [
          if (results.artists.isNotEmpty) ...[
            SliverToBoxAdapter(
              child: _sectionHeader(
                'Артисты',
                _artistsCollapsed,
                () => _toggleSection(artists: true),
                top: context.isNarrow ? 0 : 24,
              ),
            ),
            _animatedShelf(
              collapsed: _artistsCollapsed,
              child: HorizontalShelf(
                coverExtent: 144,
                padding: EdgeInsets.only(left: context.isNarrow ? 28 : 32),
                height: 240,
                itemCount: results.artists.length,
                itemBuilder: (context, i) =>
                    _ArtistSearchCard(artist: results.artists[i]),
              ),
            ),
          ],
          if (results.albums.isNotEmpty) ...[
            SliverToBoxAdapter(
              child: _sectionHeader(
                'Альбомы',
                _albumsCollapsed,
                () => _toggleSection(artists: false),
              ),
            ),
            _animatedShelf(
              collapsed: _albumsCollapsed,
              child: HorizontalShelf(
                coverExtent: 144,
                padding: EdgeInsets.only(left: context.isNarrow ? 28 : 32),
                height: 240,
                itemCount: results.albums.length,
                itemBuilder: (context, i) =>
                    _AlbumSearchCard(album: results.albums[i], size: 160),
              ),
            ),
          ],
          if (results.tracks.isNotEmpty) ...[
            const SliverToBoxAdapter(child: CommonSectionTitle(title: 'Треки')),
            AppSliverTrackList(
              itemCount: results.tracks.length,
              addAutomaticKeepAlives: false,
              padding: EdgeInsets.symmetric(
                horizontal: context.horizontalPadding,
                vertical: 4,
              ),
              itemBuilder: (context, i) => _TrackSearchTile(
                track: results.tracks[i],
                tracks: results.tracks,
              ),
            ),
            const SliverToBoxAdapter(child: SizedBox(height: 40)),
          ],
          const SliverPadding(padding: EdgeInsets.only(bottom: 140)),
        ],
      ),
    );
  }
}

class _TrackSearchTile extends StatelessWidget {
  final SimpleTrackDto track;

  final List<SimpleTrackDto> tracks;
  const _TrackSearchTile({required this.track, required this.tracks});

  @override
  Widget build(BuildContext context) {
    return CommonTrackTile(
      trackId: track.id,
      isDisliked: track.isDisliked,
      isExplicit: track.isExplicit,
      title: track.title,
      version: track.version,
      artists: track.artists,
      albumId: track.albumId,
      leading: AppCover(coverUrl: track.coverUrl),
      durationMs: track.durationMs,
      isLiked: track.isLiked,
      onTap: () => PlaybackController.playTrackList(tracks, track.id),
      onTitleTap: () {
        if (track.albumId != null) {
          navigateTo(AppSection.album, track.albumId);
        }
      },
    );
  }
}

class _AlbumSearchCard extends StatelessWidget {
  final SimpleAlbumDto album;
  final double size;

  const _AlbumSearchCard({required this.album, required this.size});

  @override
  Widget build(BuildContext context) {
    return CommonMediaCard(
      title: album.title,
      contentPadding: const EdgeInsets.fromLTRB(0, 8, 16, 8),
      size: size,
      artists: album.artists,
      coverUrl: album.coverUrl,
      onTap: () => navigateTo(AppSection.album, album.id),
    );
  }
}

class _ArtistSearchCard extends StatelessWidget {
  final SimpleArtistDto artist;

  const _ArtistSearchCard({required this.artist});

  @override
  Widget build(BuildContext context) {
    return CommonMediaCard(
      title: artist.name,
      contentPadding: const EdgeInsets.fromLTRB(0, 8, 16, 8),
      subtitle: formatArtistCardSubtitle(
        monthlyListeners: artist.monthlyListeners,
        likesCount: artist.likesCount,
      ),
      coverUrl: artist.coverUrl,
      isCircle: true,
      onTap: () => navigateTo(AppSection.artist, artist.id),
    );
  }
}
