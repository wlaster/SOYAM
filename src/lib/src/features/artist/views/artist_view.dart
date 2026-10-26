import 'dart:async';

import 'package:m3e_core/m3e_core.dart';
import 'package:material_ui/material_ui.dart';
import 'package:signals_flutter/signals_flutter.dart';
import 'package:yayma/src/app/session.dart';
import 'package:yayma/src/features/artist/views/widgets/artist_audience.dart';
import 'package:yayma/src/features/core/providers/navigation_provider.dart';
import 'package:yayma/src/features/core/providers/notification_provider.dart';
import 'package:yayma/src/features/core/views/widgets/app_cover.dart';
import 'package:yayma/src/features/core/views/widgets/common_ui.dart';
import 'package:yayma/src/features/core/views/widgets/horizontal_shelf.dart';
import 'package:yayma/src/features/core/views/widgets/media_card.dart';
import 'package:yayma/src/features/core/views/widgets/responsive.dart';
import 'package:yayma/src/features/core/views/widgets/smooth_scroll.dart';
import 'package:yayma/src/features/core/views/widgets/tab_visibility.dart';
import 'package:yayma/src/features/core/views/widgets/track_list.dart';
import 'package:yayma/src/features/core/views/widgets/track_tile.dart';
import 'package:yayma/src/features/library/providers/library_provider.dart';
import 'package:yayma/src/features/playback/providers/playback_provider.dart';
import 'package:yayma/src/rust/api/content.dart' as rust;
import 'package:yayma/src/rust/api/models.dart';

class ArtistView extends StatefulWidget {
  final String? artistId;
  const ArtistView({super.key, this.artistId});

  @override
  State<ArtistView> createState() => _ArtistViewState();
}

class _ArtistViewState extends State<ArtistView> {
  final FlutterSignal<List<SimpleTrackDto>> _tracks =
      signal<List<SimpleTrackDto>>([]);
  final FlutterSignal<SimpleArtistDto?> _artist = signal<SimpleArtistDto?>(
    null,
  );
  final FlutterSignal<List<SimpleAlbumDto>> _albums =
      signal<List<SimpleAlbumDto>>([]);
  final FlutterSignal<bool> _isLoading = signal<bool>(false);
  final FlutterSignal<String?> _isError = signal<String?>(null);
  final FlutterSignal<int> _totalTracks = signal<int>(0);
  int _currentPage = 0;
  static const _pageSize = 30;

  late final SmoothScrollController _scrollController;
  EffectCleanup? _refreshSub;
  var _lastRefreshSeen = 0;
  var _audienceRevision = 0;

  @override
  void initState() {
    super.initState();
    _scrollController = SmoothScrollController(debugLabel: 'artist')
      ..addListener(_onScroll);
    unawaited(_loadInitial());
    unawaited(refreshLikedArtists());
    _lastRefreshSeen = refreshRequestSignal.peek();
    _refreshSub = refreshRequestSignal.subscribe((count) {
      if (count == _lastRefreshSeen) return;
      _lastRefreshSeen = count;
      final nav = currentNavStateSignal.peek();
      if (nav.section == AppSection.artist && nav.id == widget.artistId) {
        unawaited(_handleRefresh());
      }
    });
  }

  @override
  void dispose() {
    _refreshSub?.call();
    _scrollController.dispose();
    super.dispose();
  }

  void _onScroll() {
    if (_scrollController.position.pixels >=
            _scrollController.position.maxScrollExtent - 500 &&
        !_isLoading.value &&
        _tracks.value.length < _totalTracks.value) {
      unawaited(_loadMore());
    }
  }

  Future<void> _loadInitial() async {
    final id = widget.artistId;
    if (id == null) return;

    _isLoading.value = true;
    _isError.value = null;

    try {
      final details = await rust.getArtistDetails(
        ctx: appContextSignal.value!,
        artistId: id,
        page: 0,
        pageSize: _pageSize,
      );
      if (details != null) {
        _artist.value = SimpleArtistDto(
          id: details.id,
          name: details.name,
          coverUrl: details.coverUrl,
          likesCount: details.likesCount,
          monthlyListeners: details.monthlyListeners,
        );
        _tracks.value = details.tracks;
        _albums.value = details.albums;
        _totalTracks.value = details.totalTracks;
        _currentPage = 0;
      } else {
        _isError.value = 'Артист не найден';
      }
    } on Object catch (e) {
      _isError.value = e.toString();
    } finally {
      _isLoading.value = false;
    }
  }

  Future<void> _handleRefresh() async {
    setState(() => _audienceRevision++);
    await Future.wait([_loadInitial(), refreshLikedArtists()]);
  }

  Future<void> _loadMore() async {
    final id = widget.artistId;
    if (id == null) return;

    _isLoading.value = true;
    _currentPage++;

    try {
      final details = await rust.getArtistDetails(
        ctx: appContextSignal.value!,
        artistId: id,
        page: _currentPage,
        pageSize: _pageSize,
      );
      if (details != null) {
        _tracks.value = [..._tracks.value, ...details.tracks];
      }
    } on Object catch (e) {
      debugPrint('Error loading more tracks: $e');
    } finally {
      _isLoading.value = false;
    }
  }

  Future<void> _toggleArtistLike(SimpleArtistDto artist, bool isLiked) async {
    final success = isLiked
        ? await removeLikedArtistAction(artist.id)
        : await addLikedArtistAction(artist.id);

    if (!mounted) return;
    if (success) {
      final current = likedArtistsSignal.value;
      if (isLiked) {
        likedArtistsSignal.value = current
            .where((a) => a.id != artist.id)
            .toList();
      } else if (!current.any((a) => a.id == artist.id)) {
        likedArtistsSignal.value = [
          SimpleArtistDto(
            id: artist.id,
            name: artist.name,
            coverUrl: artist.coverUrl,
            likesCount: artist.likesCount,
            monthlyListeners: artist.monthlyListeners,
          ),
          ...current,
        ];
      }

      showAppSuccess(
        isLiked
            ? 'Исполнитель удалён из любимых'
            : 'Исполнитель добавлен в любимые',
      );
    } else {
      showAppError('Ошибка при обновлении любимых исполнителей');
    }
  }

  @override
  Widget build(BuildContext context) {
    if (widget.artistId == null) {
      return const Center(child: Text('Артист не выбран'));
    }

    return SignalBuilder(
      builder: (context) {
        final cs = Theme.of(context).colorScheme;
        // No subscriptions while the screen is hidden (local signals keep
        // loading in the background, the UI just doesn't rebuild for it).
        if (visibleSignal(context, _isError) != null) {
          return CommonErrorWidget(error: visibleSignal(context, _isError)!);
        }

        final artist = visibleSignal(context, _artist);
        if (artist == null && visibleSignal(context, _isLoading)) {
          return const CommonLoadingWidget();
        }

        if (artist == null) {
          return const Center(child: Text('Артист не найден'));
        }

        final tracks = visibleSignal(context, _tracks);
        final albums = visibleSignal(context, _albums);
        final isLiked = visibleSignal(
          context,
          likedArtistsSignal,
        ).any((likedArtist) => likedArtist.id == artist.id);
        final likesText = artist.likesCount != null
            ? '${formatCompactAudience(artist.likesCount!)} в любимых'
            : null;

        return CommonDetailSliverLayout(
          controller: _scrollController,
          onRefresh: _handleRefresh,
          header: CommonDetailHeader(
            compactSpacing: true,
            titleOffsetY: -8,
            title: artist.name,
            subtitleWidget: ArtistAudience(
              key: ValueKey((artist.id, _audienceRevision)),
              artistId: artist.id,
              yandexListeners: artist.monthlyListeners,
            ),
            secondarySubtitle: likesText,
            coverUrl: artist.coverUrl,
            coverSize: 200,
            isCircle: true,
            actions: [
              IconButton(
                onPressed: () =>
                    unawaited(PlaybackController.playArtist(artist.id)),
                tooltip: 'Слушать',
                icon: const Icon(Icons.play_arrow_rounded),
                style: IconButton.styleFrom(
                  minimumSize: const Size(56, 56),
                  iconSize: 26,
                  backgroundColor: cs.primary,
                  foregroundColor: cs.onPrimary,
                ),
              ),
              IconButton(
                onPressed: () => unawaited(_toggleArtistLike(artist, isLiked)),
                tooltip: isLiked ? 'В любимых' : 'В любимые',
                icon: Icon(
                  isLiked
                      ? Icons.favorite_rounded
                      : Icons.favorite_border_rounded,
                ),
                style: IconButton.styleFrom(
                  minimumSize: const Size(56, 56),
                  iconSize: 26,
                  backgroundColor: isLiked
                      ? cs.primary
                      : cs.onSurface.withValues(alpha: 0.1),
                  foregroundColor: isLiked ? cs.onPrimary : cs.onSurface,
                  side: isLiked ? null : BorderSide(color: cs.outlineVariant),
                ),
              ),
            ],
          ),
          slivers: [
            if (albums.isNotEmpty) ...[
              SliverToBoxAdapter(
                child: CommonSectionTitle(
                  title: 'Альбомы',
                  padding: EdgeInsets.fromLTRB(
                    context.horizontalPadding,
                    24,
                    context.horizontalPadding,
                    16,
                  ),
                ),
              ),
              SliverToBoxAdapter(
                child: HorizontalShelf(
                  height: (context.isNarrow ? 132.0 : 160.0) + 80,
                  itemCount: albums.length,
                  itemBuilder: (context, i) {
                    final album = albums[i];
                    return CommonMediaCard(
                      contentPadding: const EdgeInsets.fromLTRB(0, 8, 16, 8),
                      title: album.title,
                      subtitle: album.year?.toString(),
                      coverUrl: album.coverUrl,
                      size: context.isNarrow ? 132 : 160,
                      onTap: () => navigateTo(AppSection.album, album.id),
                    );
                  },
                ),
              ),
            ],
            SliverToBoxAdapter(
              child: CommonSectionTitle(
                title: 'Популярные треки (${_totalTracks.value})',
                padding: EdgeInsets.fromLTRB(
                  context.horizontalPadding,
                  24,
                  context.horizontalPadding,
                  16,
                ),
              ),
            ),
            AppSliverTrackList(
              itemCount: tracks.length,
              addAutomaticKeepAlives: false,
              padding: EdgeInsets.symmetric(
                horizontal: context.horizontalPadding,
                vertical: 4,
              ),
              itemBuilder: (context, index) {
                final track = tracks[index];
                return CommonTrackTile(
                  trackId: track.id,
                  title: track.title,
                  version: track.version,
                  artists: track.artists,
                  albumId: track.albumId,
                  leading: AppCover(
                    coverUrl: track.coverUrl,
                    size: 48,
                    borderRadius: 4,
                  ),
                  trailing: Text(
                    formatDuration(track.durationMs),
                    style: TextStyle(
                      color: cs.onSurface.withValues(alpha: 0.38),
                    ),
                  ),
                  onTap: () => PlaybackController.playTrack(track.id),
                  onTitleTap: () {
                    if (track.albumId != null) {
                      navigateTo(AppSection.album, track.albumId);
                    }
                  },
                );
              },
            ),
            if (_isLoading.value)
              const SliverToBoxAdapter(
                child: Padding(
                  padding: EdgeInsets.all(20),
                  child: Center(child: M3ELoadingIndicator()),
                ),
              ),
          ],
        );
      },
    );
  }
}
