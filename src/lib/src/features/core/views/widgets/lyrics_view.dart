import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:m3e_core/m3e_core.dart';
import 'package:material_ui/material_ui.dart';
import 'package:signals_flutter/signals_flutter.dart';
import 'package:yayma/src/features/core/theme/app_tokens.dart';
import 'package:yayma/src/features/core/views/widgets/app_context_menu.dart';
import 'package:yayma/src/features/core/views/widgets/common_ui.dart';
import 'package:yayma/src/features/core/views/widgets/tab_visibility.dart';
import 'package:yayma/src/features/core/views/widgets/track_title.dart';
import 'package:yayma/src/features/playback/providers/lyrics_provider.dart';
import 'package:yayma/src/features/playback/providers/playback_provider.dart';

/// Shared timing for the active-line transition. Opacity, scale, blur, the
/// scroll that brings the line to the
/// centre all run on this, so nothing arrives out of step with the rest.
const Duration _lyricTransitionDuration = Duration(milliseconds: 400);
const Curve _lyricTransitionCurve = Curves.easeOutCubic;

/// Shared type treatment for lyric text,
/// which differ only in color, weight, and an optional highlight glow.
TextStyle _lyricTextStyle({
  required Color color,
  required FontWeight fontWeight,
  Shadow? glow,
}) {
  return TextStyle(
    color: color,
    fontSize: Platform.isAndroid ? 32 : 48,
    fontWeight: fontWeight,
    letterSpacing: -2.2,
    height: 1.1,
    shadows: [
      ?glow,
      const Shadow(color: Colors.black45, blurRadius: 10, offset: Offset(0, 4)),
    ],
  );
}

class LyricsWidget extends StatefulWidget {
  final String trackId;
  final bool visible;
  final FutureSignal<LyricsResult>? resultSignal;
  const LyricsWidget({
    required this.trackId,
    required this.visible,
    this.resultSignal,
    super.key,
  });
  @override
  State<LyricsWidget> createState() => _LyricsWidgetState();
}

class _LyricsWidgetState extends State<LyricsWidget> {
  final ScrollController _scrollController = ScrollController();
  final FlutterSignal<int> _activeIndexSignal = signal<int>(-1);
  final FlutterSignal<String> _trackIdSignal = signal<String>('');
  final FlutterSignal<bool> _visibleSignal = signal<bool>(false);

  bool _initialScrollDone = false;
  List<double> _lineOffsets = [];
  double? _viewportTop;
  FutureSignal<LyricsResult> get _result =>
      widget.resultSignal ?? lyricsSignal(_trackIdSignal.value);

  EffectCleanup? _loadingTrackingCleanup;
  EffectCleanup? _progressSubscriptionCleanup;

  @override
  void initState() {
    super.initState();
    _initialScrollDone = !widget.visible;
    _trackIdSignal.value = widget.trackId;
    _visibleSignal.value = widget.visible;
    if (widget.resultSignal == null) unawaited(refreshLyricsProviderSettings());

    _progressSubscriptionCleanup = _setupProgressSubscription();
    _loadingTrackingCleanup = _setupLoadingTracking();
  }

  EffectCleanup _setupLoadingTracking() {
    return effect(() {
      if (!_visibleSignal.value) {
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted) lyricsSuppressDimSignal.value = false;
        });
        return;
      }

      final lyricsState = _result.value;
      final isLoading = lyricsState.isLoading;
      final isEmpty = lyricsState.value?.items.isEmpty ?? false;
      final suppressDim = isLoading || isEmpty;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) lyricsSuppressDimSignal.value = suppressDim;
      });
    });
  }

  EffectCleanup _setupProgressSubscription() {
    return effect(() {
      if (!_visibleSignal.value) return;
      final value = _result.value;
      if (!value.hasValue || !value.value!.isSynced) return;
      final lines = value.value!.items;
      final index = activeLyricIndex(
        lines,
        trackProgressSignal.value.positionMs.toInt() + 400,
      );
      if (_activeIndexSignal.value != index) _activeIndexSignal.value = index;
    });
  }

  @override
  void didUpdateWidget(LyricsWidget oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.trackId != oldWidget.trackId) {
      _initialScrollDone = !widget.visible;
      _activeIndexSignal.value = -1;
      hideLyricsOverlaySignal.value = false;

      _trackIdSignal.value = widget.trackId;
      _visibleSignal.value = widget.visible;
    } else if (widget.visible != oldWidget.visible) {
      _visibleSignal.value = widget.visible;
      if (widget.visible) {
        _initialScrollDone = false;
      }
    }
  }

  @override
  void dispose() {
    _progressSubscriptionCleanup?.call();
    _loadingTrackingCleanup?.call();
    _scrollController.dispose();
    super.dispose();
  }

  double get _rowHeight => Platform.isAndroid ? 60.0 : 110.0;

  void _scrollToIndex(int index) {
    if (_scrollController.hasClients) {
      final targetScroll =
          (index < _lineOffsets.length
                  ? _lineOffsets[index]
                  : index * _rowHeight)
              .clamp(0.0, _scrollController.position.maxScrollExtent);

      if (!_initialScrollDone) {
        _initialScrollDone = true;
        _scrollController.jumpTo(targetScroll);
      } else {
        // Same timing as the rows themselves, and an ease-*out* curve on
        // purpose: lines can follow each other faster than the animation
        // lasts, and a new `animateTo` restarts from zero velocity — with an
        // ease-in the scroll would visibly stall at every such hand-off.
        _scrollController.animateTo(
          targetScroll,
          duration: _lyricTransitionDuration,
          curve: _lyricTransitionCurve,
        );
      }
    }
  }

  Widget _sourceControls(LyricsResult? result) {
    final mode = result == null || result.items.isEmpty
        ? ''
        : !result.isSynced
        ? 'Без синхронизации · прокрутка вручную'
        : 'По строкам';
    return Column(
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: Padding(
            padding: EdgeInsets.only(left: Platform.isAndroid ? 0 : 20),
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: Colors.black.withValues(alpha: 0.18),
                borderRadius: BorderRadius.circular(18),
                border: Border.all(
                  color: Theme.of(
                    context,
                  ).colorScheme.onSurface.withValues(alpha: 0.12),
                ),
              ),
              child: AppContextMenu<String>(
                key: const ValueKey('lyrics_source'),
                borderRadius: BorderRadius.circular(18),
                items: [
                  for (final entry in lyricsSources.entries)
                    if (entry.key == 'auto' ||
                        enabledLyricsSourcesSignal.value.contains(entry.key))
                      AppContextMenuItem(
                        value: entry.key,
                        label: entry.key == 'auto' ? 'Авто' : entry.value,
                        isSelected: lyricsSourceSignal.value == entry.key,
                      ),
                ],
                onSelected: (source) {
                  _initialScrollDone = false;
                  hideLyricsOverlaySignal.value = false;
                  if (lyricsSourceSignal.value == source) {
                    retryLyrics(widget.trackId);
                  } else {
                    lyricsSourceSignal.value = source;
                  }
                },
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 9,
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      const Icon(Icons.lyrics_rounded, size: 18),
                      const SizedBox(width: 8),
                      Text(
                        lyricsSourceSignal.value == 'auto'
                            ? 'Авто'
                            : lyricsSources[lyricsSourceSignal.value] ?? 'Авто',
                      ),
                      const SizedBox(width: 8),
                      const Icon(Icons.expand_more_rounded, size: 18),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
        if (mode.isNotEmpty)
          Padding(
            padding: const EdgeInsets.only(bottom: 8),
            child: _FadingSourceLabel(
              key: ValueKey(
                '${widget.trackId}_${lyricsSourceSignal.value}_${identityHashCode(result)}',
              ),
              text: '${result!.providerName} · $mode',
            ),
          ),
      ],
    );
  }

  Widget _emptyLyrics({String? error}) => Center(
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          error == null
              ? 'Текст не найден в выбранном источнике или сайт недоступен.\nПопробуйте LRCLIB, Genius или повторите загрузку.'
              : 'Не удалось загрузить текст.\n$error',
          textAlign: TextAlign.center,
        ),
        const SizedBox(height: 16),
        TextButton.icon(
          onPressed: () => retryLyrics(widget.trackId),
          icon: const Icon(Icons.refresh),
          label: const Text('Повторить'),
        ),
      ],
    ),
  );

  @override
  Widget build(BuildContext context) {
    if (!widget.visible) return const SizedBox.shrink();
    return SignalBuilder(
      builder: (context) {
        final lyricsAsync = _result.value;
        return Column(
          children: [
            _sourceControls(lyricsAsync.value),
            Expanded(
              child: lyricsAsync.map(
                data: (result) {
                  final lines = result.items;
                  if (lines.isEmpty) return _emptyLyrics();
                  if (!result.isSynced) {
                    return ShaderMask(
                      shaderCallback: (rect) => const LinearGradient(
                        begin: Alignment.topCenter,
                        end: Alignment.bottomCenter,
                        colors: [
                          Colors.transparent,
                          Colors.white,
                          Colors.white,
                          Colors.transparent,
                        ],
                        stops: [0, 0.12, 0.72, 1],
                      ).createShader(rect),
                      blendMode: BlendMode.dstIn,
                      child: ListView.builder(
                        key: PageStorageKey(
                          '${widget.trackId}_${result.providerName}_plain',
                        ),
                        primary: false,
                        padding: const EdgeInsets.fromLTRB(24, 8, 24, 40),
                        itemCount: lines.length,
                        itemBuilder: (context, index) => Padding(
                          padding: const EdgeInsets.only(bottom: 12),
                          child: Text(
                            (lines[index] as LyricLine).text,
                            textAlign: TextAlign.center,
                            style: _lyricTextStyle(
                              color: Theme.of(context).colorScheme.onSurface,
                              fontWeight: FontWeight.w800,
                            ),
                          ),
                        ),
                      ),
                    );
                  }
                  return Stack(
                    children: [
                      LayoutBuilder(
                        builder: (context, constraints) {
                          final viewportHeight = constraints.maxHeight;
                          WidgetsBinding.instance.addPostFrameCallback((_) {
                            if (!mounted) return;
                            final box = context.findRenderObject();
                            if (box is RenderBox && box.hasSize) {
                              final top = box.localToGlobal(Offset.zero).dy;
                              if (_viewportTop == null ||
                                  (top - _viewportTop!).abs() > .5) {
                                setState(() => _viewportTop = top);
                              }
                            }
                          });
                          final textWidth =
                              (constraints.maxWidth -
                                      (Platform.isAndroid ? 32 : 96))
                                  .clamp(1.0, double.infinity);
                          final heights = lines.map((item) {
                            if (item is! LyricLine) return _rowHeight;
                            final painter = TextPainter(
                              text: TextSpan(
                                text: item.text,
                                style: _lyricTextStyle(
                                  color: Colors.white,
                                  fontWeight: FontWeight.w900,
                                ),
                              ),
                              textDirection: Directionality.of(context),
                              textScaler: MediaQuery.textScalerOf(context),
                            )..layout(maxWidth: textWidth);
                            final height = (painter.height + 36).clamp(
                              _rowHeight,
                              double.infinity,
                            );
                            painter.dispose();
                            return height;
                          }).toList();
                          final anchor = Platform.isAndroid
                              ? viewportHeight * .43
                              : (MediaQuery.sizeOf(context).height / 2 -
                                        (_viewportTop ?? 100))
                                    .clamp(0.0, viewportHeight);
                          var offset = 0.0;
                          _lineOffsets = [
                            for (var i = 0; i < heights.length; i++)
                              (() {
                                final center =
                                    offset +
                                    heights[i] / 2 +
                                    (anchor - heights.first / 2).clamp(
                                      0.0,
                                      double.infinity,
                                    ) -
                                    anchor;
                                offset += heights[i];
                                return center;
                              })(),
                          ];

                          return SignalBuilder(
                            builder: (context) {
                              final activeIndex = _activeIndexSignal.value;

                              if (activeIndex != -1 &&
                                  (Platform.isAndroid ||
                                      _viewportTop != null)) {
                                WidgetsBinding.instance.addPostFrameCallback((
                                  _,
                                ) {
                                  _scrollToIndex(activeIndex);
                                });
                              }

                              return ShaderMask(
                                shaderCallback: (rect) => const LinearGradient(
                                  begin: Alignment.topCenter,
                                  end: Alignment.bottomCenter,
                                  colors: [
                                    Colors.transparent,
                                    Colors.white,
                                    Colors.white,
                                    Colors.transparent,
                                  ],
                                  stops: [0.0, 0.25, 0.75, 1.0],
                                ).createShader(rect),
                                blendMode: BlendMode.dstIn,
                                child: ScrollConfiguration(
                                  behavior: ScrollConfiguration.of(
                                    context,
                                  ).copyWith(scrollbars: false),
                                  child: ListView.builder(
                                    controller: _scrollController,
                                    physics:
                                        const NeverScrollableScrollPhysics(),
                                    itemCount: lines.length,
                                    padding: EdgeInsets.only(
                                      top: (anchor - heights.first / 2).clamp(
                                        0.0,
                                        double.infinity,
                                      ),
                                      bottom: (viewportHeight - anchor).clamp(
                                        0.0,
                                        double.infinity,
                                      ),
                                    ),
                                    itemBuilder: (context, index) {
                                      final item = lines[index];
                                      final isActive = index == activeIndex;
                                      final distance = (index - activeIndex)
                                          .abs();

                                      return Semantics(
                                        key: ValueKey('lyric_$index'),
                                        selected: isActive,
                                        child: SizedBox(
                                          height: heights[index],
                                          child: _LyricRow(
                                            key: ValueKey(
                                              '${widget.trackId}_$index',
                                            ),
                                            item: item,
                                            isActive: isActive,
                                            distance: distance,
                                          ),
                                        ),
                                      );
                                    },
                                  ),
                                ),
                              );
                            },
                          );
                        },
                      ),
                    ],
                  );
                },
                loading: () => const _LyricsLoadingIndicator(),
                error: (Object e, _) => _emptyLyrics(error: e.toString()),
              ),
            ),
          ],
        );
      },
    );
  }
}

/// M3-Expressive loading state for the lyrics panel: the real Android M3
/// `LoadingIndicator` (ported by `m3e_core`), which morphs between
/// `RoundedPolygon` shapes with spring physics — near the top of the screen
/// instead of a centered spinner, shown while the text is being fetched (no
/// dark scrim behind it — see [lyricsSuppressDimSignal] in `layout.dart`).
class _FadingSourceLabel extends StatefulWidget {
  final String text;
  const _FadingSourceLabel({required this.text, super.key});

  @override
  State<_FadingSourceLabel> createState() => _FadingSourceLabelState();
}

class _FadingSourceLabelState extends State<_FadingSourceLabel> {
  Timer? _timer;
  bool _visible = true;

  @override
  void initState() {
    super.initState();
    _timer = Timer(const Duration(seconds: 2), () {
      if (mounted) setState(() => _visible = false);
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedOpacity(
    opacity: _visible ? 1 : 0,
    duration: const Duration(milliseconds: 200),
    child: Text(widget.text),
  );
}

class _LyricsLoadingIndicator extends StatelessWidget {
  const _LyricsLoadingIndicator();

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: Alignment.topCenter,
      child: Padding(
        padding: const EdgeInsets.only(top: 32),
        child: M3ELoadingIndicator(
          color: Theme.of(context).colorScheme.primary,
        ),
      ),
    );
  }
}

/// [ImageFiltered] with an animated blur sigma. `ImageFilter.blur` is a plain
/// value, so changing it rebuilds with the new radius instantly — this tweens
/// the sigma over [_lyricTransitionDuration] instead. The filter (and its
/// `saveLayer`) is skipped entirely once the blur is effectively zero.
class _AnimatedBlur extends StatelessWidget {
  final double sigma;
  final Widget child;

  const _AnimatedBlur({required this.sigma, required this.child});

  @override
  Widget build(BuildContext context) {
    return TweenAnimationBuilder<double>(
      tween: Tween<double>(end: sigma),
      duration: _lyricTransitionDuration,
      curve: _lyricTransitionCurve,
      child: child,
      builder: (context, value, child) {
        if (value < 0.05) return child!;
        return ImageFiltered(
          imageFilter: ui.ImageFilter.blur(
            sigmaX: value,
            sigmaY: value,
            tileMode: TileMode.decal,
          ),
          child: child,
        );
      },
    );
  }
}

class _LyricRow extends StatelessWidget {
  final LyricItem item;
  final bool isActive;
  final int distance;

  const _LyricRow({
    required this.item,
    required this.isActive,
    required this.distance,
    super.key,
  });

  @override
  Widget build(BuildContext context) {
    final onSurface = Theme.of(context).colorScheme.onSurface;
    if (item is LyricTimer) {
      return _LyricTimerWidget(item: item as LyricTimer, isActive: isActive);
    }

    final line = item as LyricLine;
    var opacity = 1.0;
    var scale = 1.0;
    var blur = 0.0;

    if (isActive) {
      opacity = 1.0;
      scale = 1.0;
      blur = 0.0;
    } else {
      if (distance == 1) {
        opacity = 0.4;
        scale = 0.94;
        blur = 1.0;
      } else if (distance == 2) {
        opacity = 0.15;
        scale = 0.9;
        blur = 2.0;
      } else {
        // No blur beyond distance 2: at 0.05 opacity the row is barely
        // visible anyway, so skip the extra `ImageFiltered` saveLayer —
        // rows this far out are usually the majority of what's on screen,
        // and each one stacks another offscreen pass during the transition.
        opacity = 0.05;
        scale = 0.86;
        blur = 0.0;
      }
    }

    return RepaintBoundary(
      child: Center(
        child: Container(
          width: double.infinity,
          padding: EdgeInsets.symmetric(
            horizontal: Platform.isAndroid ? 16 : 48,
          ),
          alignment: Alignment.center,
          child: AnimatedScale(
            duration: _lyricTransitionDuration,
            curve: _lyricTransitionCurve,
            scale: scale,
            child: AnimatedOpacity(
              duration: _lyricTransitionDuration,
              curve: _lyricTransitionCurve,
              opacity: opacity,
              child: _AnimatedBlur(
                sigma: blur,
                child: Text(
                  line.text,
                  textAlign: TextAlign.center,
                  style: _lyricTextStyle(
                    color: onSurface,
                    fontWeight: isActive ? FontWeight.w900 : FontWeight.w800,
                    glow: isActive
                        ? Shadow(
                            color: onSurface.withValues(alpha: 0.3),
                            blurRadius: 20,
                          )
                        : null,
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _LyricTimerWidget extends StatefulWidget {
  final LyricTimer item;
  final bool isActive;

  const _LyricTimerWidget({required this.item, required this.isActive});

  @override
  State<_LyricTimerWidget> createState() => _LyricTimerWidgetState();
}

class _LyricTimerWidgetState extends State<_LyricTimerWidget>
    with SingleTickerProviderStateMixin {
  late final AnimationController _pulseController;

  @override
  void initState() {
    super.initState();
    _pulseController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1000),
    );
    _pulseController.repeat(reverse: true);
  }

  @override
  void dispose() {
    _pulseController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return SignalBuilder(
      builder: (context) {
        final cs = Theme.of(context).colorScheme;
        // No progress subscription while the hosting tab is hidden.
        final progress = visibleSignal(context, trackProgressSignal);
        final currentMs = progress.positionMs.toInt();
        final remainingMs =
            (widget.item.time.inMilliseconds +
                widget.item.duration.inMilliseconds) -
            currentMs;
        final showDots =
            widget.isActive &&
            remainingMs > 0 &&
            (remainingMs / 1000).ceil() <= 5;

        if (!showDots) return const SizedBox.shrink();

        return Center(
          child: Row(
            mainAxisSize: MainAxisSize.min,
            mainAxisAlignment: MainAxisAlignment.center,
            children: List.generate(3, (index) {
              final dotValue = (remainingMs / 1000) - (2 - index);
              final active = dotValue > 0;

              return AnimatedBuilder(
                animation: _pulseController,
                builder: (context, child) {
                  final pulse = active
                      ? (_pulseController.value * 0.15 + 1.0)
                      : 1.0;
                  return Transform.scale(scale: pulse, child: child);
                },
                child: Container(
                  margin: const EdgeInsets.symmetric(horizontal: 12),
                  width: 14,
                  height: 14,
                  decoration: BoxDecoration(
                    color: cs.onSurface.withValues(alpha: active ? 0.9 : 0.1),
                    shape: BoxShape.circle,
                    boxShadow: active
                        ? [
                            BoxShadow(
                              color: cs.onSurface.withValues(alpha: 0.2),
                              blurRadius: 8,
                            ),
                          ]
                        : null,
                  ),
                ),
              );
            }),
          ),
        );
      },
    );
  }
}

class LyricsReaderDialog extends StatefulWidget {
  final String trackId;
  final String title;

  /// Mobile layout: rendered inside a modal bottom sheet instead of a dialog.
  final bool bottomSheet;
  final ScrollController? scrollController;

  const LyricsReaderDialog({
    required this.trackId,
    required this.title,
    this.bottomSheet = false,
    this.scrollController,
    super.key,
  });

  static void show(BuildContext context, String trackId, String title) {
    if (Platform.isAndroid) {
      unawaited(
        showModalBottomSheet<void>(
          context: context,
          useSafeArea: true,
          isScrollControlled: true,
          showDragHandle: true,
          shape: const RoundedRectangleBorder(
            borderRadius: BorderRadius.vertical(
              top: Radius.circular(AppRadius.xl),
            ),
          ),
          builder: (context) => DraggableScrollableSheet(
            expand: false,
            initialChildSize: 0.85,
            minChildSize: 0.5,
            maxChildSize: 0.95,
            builder: (context, scrollController) => LyricsReaderDialog(
              trackId: trackId,
              title: title,
              bottomSheet: true,
              scrollController: scrollController,
            ),
          ),
        ),
      );
      return;
    }

    unawaited(
      showDialog<void>(
        context: context,
        builder: (context) =>
            LyricsReaderDialog(trackId: trackId, title: title),
      ),
    );
  }

  @override
  State<LyricsReaderDialog> createState() => _LyricsReaderDialogState();
}

class _LyricsReaderDialogState extends State<LyricsReaderDialog> {
  @override
  Widget build(BuildContext context) {
    return SignalBuilder(
      builder: (context) {
        final cs = Theme.of(context).colorScheme;
        final lyricsAsync = lyricsSignal(widget.trackId).value;

        Widget buildLyrics() {
          return lyricsAsync.map(
            data: (result) {
              final lines = result.items.whereType<LyricLine>().toList();
              if (lines.isEmpty) {
                return Center(
                  child: Text(
                    'Текст отсутствует',
                    style: TextStyle(
                      color: cs.onSurface.withValues(alpha: 0.24),
                      fontSize: 18,
                    ),
                  ),
                );
              }
              return ListView.builder(
                controller: widget.scrollController,
                itemCount: lines.length,
                itemBuilder: (context, index) => Padding(
                  padding: const EdgeInsets.symmetric(vertical: 18),
                  child: Text(
                    lines[index].text,
                    style: TextStyle(
                      color: cs.onSurface,
                      fontSize: 26,
                      fontWeight: FontWeight.w800,
                      height: 1.3,
                      letterSpacing: -0.5,
                    ),
                  ),
                ),
              );
            },
            loading: () => const CommonLoadingWidget(),
            error: (Object e, _) => CommonErrorWidget(error: e.toString()),
          );
        }

        if (widget.bottomSheet) {
          return SafeArea(
            top: false,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(24, 0, 24, 8),
                  child: Text(
                    widget.title,
                    style: trackTitleStyle(context, weight: FontWeight.w900),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                Expanded(
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 20),
                    child: buildLyrics(),
                  ),
                ),
              ],
            ),
          );
        }

        return AppDialog(
          surfaceTintColor: Colors.transparent,
          titleWidget: Row(
            children: [
              Expanded(
                child: Text(
                  widget.title,
                  style: trackTitleStyle(context, weight: FontWeight.w900),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              IconButton(
                icon: Icon(Icons.close, color: cs.onSurfaceVariant),
                onPressed: () => Navigator.pop(context),
              ),
            ],
          ),
          content: Container(
            width: 650,
            height: 800,
            padding: const EdgeInsets.symmetric(horizontal: 20),
            child: buildLyrics(),
          ),
        );
      },
    );
  }
}
