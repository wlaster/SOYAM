import 'dart:async';
import 'dart:io';

import 'package:m3e_core/m3e_core.dart';
import 'package:material_ui/material_ui.dart';
import 'package:signals_flutter/signals_flutter.dart';
import 'package:yayma/src/features/core/theme/app_tokens.dart';
import 'package:yayma/src/features/core/views/widgets/app_context_menu.dart';
import 'package:yayma/src/features/core/views/widgets/app_cover.dart';
import 'package:yayma/src/features/core/views/widgets/common_ui.dart';
import 'package:yayma/src/features/core/views/widgets/responsive.dart';
import 'package:yayma/src/features/core/views/widgets/tab_visibility.dart';
import 'package:yayma/src/features/core/views/widgets/track_context_actions.dart';
import 'package:yayma/src/features/core/views/widgets/track_elements.dart';
import 'package:yayma/src/features/core/views/widgets/track_title.dart';
import 'package:yayma/src/features/library/providers/library_provider.dart';
import 'package:yayma/src/features/playback/providers/playback_provider.dart';
import 'package:yayma/src/rust/api/models.dart';

class CommonTrackTile extends StatefulWidget {
  final String trackId;
  final int? durationMs;
  final bool isLiked;
  final bool isExplicit;
  final bool replaceLeadingWhenPlaying;
  final String title;
  final String? version;
  final List<TrackArtistDto> artists;
  final Widget? leading;
  final Widget? trailing;
  final List<Widget>? hoverActions;
  final VoidCallback? onTap;
  final VoidCallback? onTitleTap;
  final EdgeInsetsGeometry? contentPadding;
  final String? albumId;
  final double? artistFontSize;

  /// Cover for the sheet menu header on Android, when [leading] is
  /// not a cover (e.g. the track number in an album).
  final String? sheetCoverUrl;

  const CommonTrackTile({
    required this.trackId,
    required this.title,
    required this.artists,
    super.key,
    this.durationMs,
    this.isLiked = false,
    this.isExplicit = false,
    this.replaceLeadingWhenPlaying = false,
    this.version,
    this.leading,
    this.trailing,
    this.hoverActions,
    this.onTap,
    this.onTitleTap,
    this.contentPadding,
    this.albumId,
    this.artistFontSize,
    this.sheetCoverUrl,
  });

  @override
  State<CommonTrackTile> createState() => _CommonTrackTileState();
}

class _CommonTrackTileState extends State<CommonTrackTile> {
  final ValueNotifier<bool> _isHovered = ValueNotifier(false);
  final ValueNotifier<bool> _isPressed = ValueNotifier(false);
  final ValueNotifier<bool> _isMenuOpen = ValueNotifier(false);

  @override
  void dispose() {
    _isHovered.dispose();
    _isPressed.dispose();
    _isMenuOpen.dispose();
    super.dispose();
  }

  void _handleTap() {
    widget.onTap?.call();
  }

  Widget _adjustLeading(Widget leading, bool isNarrow) {
    if (isNarrow && leading is AppCover && leading.size == 64) {
      return AppCover(
        key: leading.key,
        coverUrl: leading.coverUrl ?? leading.url,
        url: leading.url,
        localUri: leading.localUri,
        size: 48,
        circle: leading.circle,
        heroTag: leading.heroTag,
        onTap: leading.onTap,
        borderRadius: leading.borderRadius,
        radius: leading.radius,
        shape: leading.shape,
        canExpand: leading.canExpand,
        hoverEnabled: leading.hoverEnabled,
        hoverScale: leading.hoverScale,
        fit: leading.fit,
        placeholderIcon: leading.placeholderIcon,
      );
    }
    return leading;
  }

  /// Actions shared with the full player menu.
  TrackContextActions get _trackActions {
    final leading = widget.leading;
    final cover = leading is AppCover ? leading : null;
    return TrackContextActions(
      trackId: widget.trackId,
      title: widget.title,
      version: widget.version,
      artists: widget.artists,
      albumId: widget.albumId,
      coverUrl: cover?.coverUrl ?? cover?.url ?? widget.sheetCoverUrl,
      localCoverUri: cover?.localUri,
    );
  }

  /// Open the bottom menu (sheet) with actions on a track.
  Future<void> _showTrackBottomSheet() => _trackActions.showSheet(context);

  Future<void> _handleMenuAction(String value) async {
    if (!mounted) return;
    _isHovered.value = false;
    await _trackActions.handle(context, value);
  }

  @override
  Widget build(BuildContext context) {
    return SignalBuilder(
      builder: (context) {
        final isNarrow = context.isNarrow;

        final effectivePadding =
            widget.contentPadding ??
            EdgeInsets.symmetric(
              horizontal: isNarrow ? 8 : 12,
              vertical: isNarrow ? 4 : 8,
            );

        Widget buildContextMenu() {
          return SignalBuilder(
            builder: (watchContext) {
              // Menu content is built lazily here; no subscription while
              // the hosting tab is hidden.
              final playlists = visibleSignal(watchContext, playlistsSignal);
              final membershipKinds =
                  visibleSignal(
                    watchContext,
                    trackPlaylistMembershipSignal,
                  )[widget.trackId] ??
                  const <int>{};
              return AppContextMenu<String>(
                onOpen: () {
                  _isMenuOpen.value = true;
                  unawaited(refreshTrackPlaylistMembership(widget.trackId));
                },
                onClose: () {
                  _isMenuOpen.value = false;
                  _isHovered.value = false;
                },
                onSelected: (value) async {
                  if (!mounted) return;
                  await _handleMenuAction(value);
                },
                items: [
                  AppContextMenuItem(
                    label: 'Добавить в плейлист',
                    searchable: true,
                    icon: Icons.playlist_add_rounded,
                    subItems: [
                      ...playlists.map(
                        (p) => AppContextMenuItem(
                          value: 'add_to_${p.kind}',
                          label: p.title,
                          icon: Icons.library_music_rounded,
                          isSelected: membershipKinds.contains(p.kind),
                          leading: p.coverUrl == null
                              ? null
                              : AppCover(
                                  coverUrl: p.coverUrl,
                                  url: p.coverUrl,
                                  size: 32,
                                ),
                        ),
                      ),
                      const AppContextMenuItem(
                        value: 'create_playlist',
                        pinned: true,
                        label: 'Создать плейлист',
                        icon: Icons.add_rounded,
                      ),
                    ],
                  ),
                  if (widget.albumId != null)
                    const AppContextMenuItem(
                      value: 'go_to_album',
                      label: 'Перейти к альбому',
                      icon: Icons.album_rounded,
                    ),
                  const AppContextMenuItem(
                    value: 'lyrics',
                    label: 'Открыть текст',
                    icon: Icons.lyrics_rounded,
                  ),
                  const AppContextMenuItem(
                    value: 'copy_link',
                    label: 'Скопировать ссылку',
                    icon: Icons.link_rounded,
                  ),
                  const AppContextMenuItem(
                    value: 'wave',
                    label: 'Моя волна по треку',
                    icon: Icons.waves_rounded,
                  ),
                  const AppContextMenuItem(
                    value: 'about',
                    label: 'О треке',
                    icon: Icons.info_outline_rounded,
                  ),
                  const AppContextMenuItem(
                    label: 'Скачать',
                    icon: Icons.download_rounded,
                    subItems: [
                      AppContextMenuItem(
                        value: 'download_cache',
                        label: 'В кэш приложения',
                        icon: Icons.offline_bolt_rounded,
                      ),
                      AppContextMenuItem(
                        value: 'download_files',
                        label: 'В отдельный файл',
                        icon: Icons.file_download_rounded,
                      ),
                    ],
                  ),
                  if (visibleSignal(
                    watchContext,
                    downloadedTracksSignal,
                  ).contains(widget.trackId))
                    const AppContextMenuItem(
                      value: 'delete_downloaded',
                      label: 'Удалить из загрузок',
                      icon: Icons.remove_circle_outline_rounded,
                    ),
                ],
                child: IconButton(
                  icon: Icon(
                    Icons.more_vert_rounded,
                    color: Theme.of(context).colorScheme.onSurfaceVariant,
                  ),
                  onPressed: null,
                  tooltip: 'Действия',
                ),
              );
            },
          );
        }

        return Material(
          color: Colors.transparent,
          child: InkWell(
            onTap: _handleTap,
            onHover: (value) => _isHovered.value = value,
            onHighlightChanged: (value) => _isPressed.value = value,
            hoverColor: Theme.of(
              context,
            ).colorScheme.onSurface.withValues(alpha: 0.06),
            focusColor: Colors.transparent,
            highlightColor: Colors.transparent,
            splashColor: Theme.of(
              context,
            ).colorScheme.primary.withValues(alpha: 0.06),
            borderRadius: BorderRadius.circular(AppRadius.sm),
            child: ValueListenableBuilder<bool>(
              valueListenable: _isHovered,
              builder: (context, isHovered, child) {
                return ValueListenableBuilder<bool>(
                  valueListenable: _isMenuOpen,
                  builder: (context, isMenuOpen, child) {
                    final showHighlighted = isHovered || isMenuOpen;
                    return ValueListenableBuilder<bool>(
                      valueListenable: _isPressed,
                      builder: (context, isPressed, child) {
                        return AnimatedScale(
                          scale: isPressed ? 0.98 : 1.0,
                          duration: Duration(
                            milliseconds: isPressed ? 80 : 180,
                          ),
                          curve: isPressed
                              ? Curves.easeOutCubic
                              : Curves.easeOutBack,
                          child: AnimatedContainer(
                            duration: const Duration(milliseconds: 150),
                            padding: effectivePadding,
                            constraints: BoxConstraints(
                              minHeight: isNarrow ? 64 : 72,
                            ),
                            decoration: BoxDecoration(
                              color: isMenuOpen
                                  ? Theme.of(context).colorScheme.onSurface
                                        .withValues(alpha: 0.08)
                                  : Colors.transparent,
                              borderRadius: BorderRadius.circular(AppRadius.sm),
                            ),
                            child: Row(
                              children: [
                                if (widget.leading != null) ...[
                                  Padding(
                                    padding: EdgeInsets.only(
                                      left: widget.leading is AppCover ? 20 : 0,
                                    ),
                                    child: _LeadingPlayingBadge(
                                      trackId: widget.trackId,
                                      replace: widget.replaceLeadingWhenPlaying,
                                      child: _adjustLeading(
                                        widget.leading!,
                                        isNarrow,
                                      ),
                                    ),
                                  ),
                                  SizedBox(width: isNarrow ? 12 : 16),
                                ],
                                Expanded(
                                  child: Column(
                                    mainAxisSize: MainAxisSize.min,
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      Row(
                                        children: [
                                          Flexible(
                                            child: MouseRegion(
                                              cursor: widget.onTitleTap != null
                                                  ? SystemMouseCursors.click
                                                  : SystemMouseCursors.basic,
                                              child: GestureDetector(
                                                behavior: HitTestBehavior.opaque,
                                                onTap: widget.onTitleTap,
                                                child: Text(
                                                  widget.title,
                                                  style: trackTitleStyle(
                                                    context,
                                                    weight: FontWeight.bold,
                                                  ),
                                                  maxLines: 1,
                                                  overflow: TextOverflow.ellipsis,
                                                ),
                                              ),
                                            ),
                                          ),
                                          SignalBuilder(
                                            builder: (context) {
                                              // No subscriptions while the hosting tab is hidden.
                                              final isDownloading =
                                                  visibleSignal(
                                                    context,
                                                    downloadingTracksSignal,
                                                  ).contains(widget.trackId);
                                              final isDownloaded =
                                                  visibleSignal(
                                                    context,
                                                    downloadedTracksSignal,
                                                  ).contains(widget.trackId);

                                              if (!isDownloading &&
                                                  !isDownloaded) {
                                                return const SizedBox.shrink();
                                              }

                                              return Padding(
                                                padding: const EdgeInsets.only(
                                                  left: 6,
                                                ),
                                                child: isDownloading
                                                    ? M3ECircularWavyProgressIndicator(
                                                        strokeWidth: 2,
                                                        size: isNarrow
                                                            ? 12
                                                            : 14,
                                                      )
                                                    : Icon(
                                                        Icons
                                                            .download_done_rounded,
                                                        size: isNarrow
                                                            ? 14
                                                            : 16,
                                                        color: Theme.of(
                                                          context,
                                                        ).colorScheme.primary,
                                                      ),
                                              );
                                            },
                                          ),
                                          if (widget.isExplicit)
                                            const ExplicitTrackBadge(),
                                          TrackVersionWidget(
                                            version: widget.version,
                                            fontSize: isNarrow ? 12 : 14,
                                          ),
                                        ],
                                      ),
                                      const SizedBox(height: 2),
                                      ArtistNamesWidget(
                                        artists: widget.artists,
                                        fontSize:
                                            widget.artistFontSize ??
                                            (isNarrow ? 13 : 15),
                                      ),
                                    ],
                                  ),
                                ),
                                SizedBox(width: isNarrow ? 12 : 16),
                                Row(
                                  mainAxisSize: MainAxisSize.min,
                                  children: [
                                    if (!Platform.isAndroid &&
                                        widget.hoverActions != null)
                                      SizedBox(
                                        width:
                                            40.0 * widget.hoverActions!.length,
                                        child: showHighlighted
                                            ? Row(
                                                mainAxisSize: MainAxisSize.min,
                                                children: widget.hoverActions!
                                                    .map(
                                                      (action) => SizedBox(
                                                        width: 40,
                                                        height: 40,
                                                        child: action,
                                                      ),
                                                    )
                                                    .toList(),
                                              )
                                            : null,
                                      ),
                                    SignalBuilder(
                                      builder: (context) {
                                        final id = widget.trackId
                                            .split(':')
                                            .first;
                                        final liked =
                                            visibleSignal(
                                              context,
                                              trackLikeOverridesSignal,
                                            )[id] ??
                                            widget.isLiked;
                                        final pending = visibleSignal(
                                          context,
                                          pendingTrackLikesSignal,
                                        ).contains(id);
                                        return Tooltip(
                                          message: liked
                                              ? 'Убрать из любимых'
                                              : 'В любимые',
                                          child: AnimatedLikeButton(
                                            key: ValueKey('track-like-$id'),
                                            isLiked: liked,
                                            size: 20,
                                            showBackground: false,
                                            onTap: () {
                                              if (!pending) {
                                                unawaited(
                                                  PlaybackController.toggleLike(
                                                    trackId: widget.trackId,
                                                    isLiked: liked,
                                                  ),
                                                );
                                              }
                                            },
                                          ),
                                        );
                                      },
                                    ),
                                    const SizedBox(width: 8),
                                    if (widget.durationMs != null)
                                      SizedBox(
                                        width: 48,
                                        child: Text(
                                          formatDuration(widget.durationMs!),
                                          textAlign: TextAlign.end,
                                          style: TextStyle(
                                            color: Theme.of(context)
                                                .colorScheme
                                                .onSurface
                                                .withValues(alpha: 0.38),
                                          ),
                                        ),
                                      )
                                    else if (widget.trailing != null)
                                      widget.trailing!,
                                    const SizedBox(width: 8),
                                    SizedBox(
                                      width: 40,
                                      child: Platform.isAndroid
                                          ? IconButton(
                                              icon: const Icon(
                                                Icons.more_vert_rounded,
                                              ),
                                              tooltip: 'Действия',
                                              onPressed: _showTrackBottomSheet,
                                            )
                                          : showHighlighted
                                          ? buildContextMenu()
                                          : null,
                                    ),
                                  ],
                                ),
                              ],
                            ),
                          ),
                        );
                      },
                    );
                  },
                );
              },
            ),
          ),
        );
      },
    );
  }
}

/// Badge over leading (usually the cover): visible only on the current track.
/// While playing — animated equalizer, while paused — static.
class _LeadingPlayingBadge extends StatelessWidget {
  final String trackId;
  final Widget child;
  final bool replace;

  const _LeadingPlayingBadge({
    required this.trackId,
    required this.child,
    this.replace = false,
  });

  @override
  Widget build(BuildContext context) {
    return SignalBuilder(
      builder: (context) {
        // No subscriptions while the hosting tab is hidden.
        final isCurrent =
            visibleSignal(context, currentTrackIdSignal) == trackId;
        if (!isCurrent) return child;
        final isPlaying = visibleSignal(context, isPlayingSignal);
        final scheme = Theme.of(context).colorScheme;
        if (replace) {
          return Stack(
            alignment: Alignment.center,
            children: [
              Visibility(
                visible: false,
                maintainSize: true,
                maintainState: true,
                maintainAnimation: true,
                child: child,
              ),
              TrackPlayingIndicator(
                isPlaying: isPlaying,
                color: scheme.primary,
              ),
            ],
          );
        }
        return Stack(
          clipBehavior: Clip.none,
          children: [
            child,
            Positioned(
              right: -6,
              bottom: -6,
              child: Tooltip(
                message: isPlaying ? 'Сейчас играет' : 'Текущий трек на паузе',
                child: Container(
                  width: 28,
                  height: 28,
                  alignment: Alignment.center,
                  decoration: BoxDecoration(
                    color: scheme.primary,
                    borderRadius: BorderRadius.circular(10),
                    border: Border.all(color: scheme.surface, width: 2),
                    boxShadow: [
                      BoxShadow(
                        color: Colors.black.withValues(alpha: 0.3),
                        blurRadius: 6,
                        offset: const Offset(0, 2),
                      ),
                    ],
                  ),
                  child: TrackPlayingIndicator(
                    isPlaying: isPlaying,
                    height: 12,
                    color: scheme.onPrimary,
                  ),
                ),
              ),
            ),
          ],
        );
      },
    );
  }
}
